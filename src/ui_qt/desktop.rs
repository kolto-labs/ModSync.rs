use crate::{
    Guide, PatcherEngine,
    p2p::{PeerMode, PeerSettings},
    peer_sharing::{Enrollment, PauseReason, SharingOwner},
    profiles::{DesktopSettings, PatcherPreferences, Profile, SettingsFile, Store},
    telemetry::{Config, lifecycle::Lifecycle},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
};

pub const SHARING_GATE: &str =
    "Pause sharing before editing the guide, folders, or install settings.";

static LIFECYCLE: Mutex<Option<Arc<Lifecycle>>> = Mutex::new(None);
static SHARING: Mutex<Option<Arc<SharingOwner>>> = Mutex::new(None);

pub fn shutdown() {
    let sharing = SHARING.lock().ok().and_then(|mut slot| slot.take());
    if let Some(owner) = sharing {
        match Arc::try_unwrap(owner) {
            Ok(mut owner) => {
                let _ = owner.shutdown();
            }
            Err(shared) => drop(shared),
        }
    }
    let lifecycle = LIFECYCLE.lock().ok().and_then(|mut slot| slot.take());
    if let Some(lifecycle) = lifecycle {
        let _ = lifecycle.shutdown(true, true);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !lifecycle.status().stopped && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SharingAction {
    ResumeMutation,
    PauseForEdit,
    Stop,
    Resume,
}

pub struct Loaded {
    pub settings: DesktopSettings,
    pub patcher: PatcherPreferences,
    pub config: Config,
    pub warnings: Vec<String>,
    pub profiles: Vec<String>,
    pub active: Option<String>,
    pub peer: PeerSettings,
}

pub enum Event {
    Loaded(Box<Result<Loaded>>),
    SharingStarted(Result<Option<SharingOwner>>),
    SharingSettled {
        action: SharingAction,
        result: std::result::Result<(), String>,
    },
    TelemetrySaved(Box<Result<Config>>),
    KeySaved(std::result::Result<(), String>),
}

pub fn load_work(explicit: Option<PathBuf>) -> Event {
    let result = std::panic::catch_unwind(|| -> Result<Loaded> {
        let store = Store::from_settings_directory(explicit.as_deref())?;
        let migration_failed = store.nexus_api_key().is_err();
        let (mut report, patcher) = store.desktop_settings_with_patcher();
        if migration_failed {
            report.settings.nexus_mods_api_key = None;
            report.warnings.push(
                "Nexus API key migration failed; the key remains unset for this session.".into(),
            );
        }
        let mut warnings = report.warnings;
        if report.used_defaults {
            warnings.push(
                "Invalid stored settings were replaced by the typed defaults for this session."
                    .into(),
            );
        }
        let config =
            Config::load_from_settings_directory(explicit.as_deref()).unwrap_or_else(|error| {
                warnings.push(format!("Telemetry disabled: {error:#}"));
                Config {
                    is_first_run: false,
                    ..Config::default()
                }
            });
        let profiles = store
            .list()
            .map(|list| list.into_iter().map(|profile| profile.name).collect())
            .unwrap_or_default();
        let active = store.active_name().unwrap_or(None);
        let peer = SettingsFile::resolve(explicit.as_deref())
            .and_then(|file| file.load())
            .map(|stored| PeerSettings::from_settings(&stored))
            .unwrap_or_default();
        Ok(Loaded {
            settings: report.settings,
            patcher,
            config,
            warnings,
            profiles,
            active,
            peer,
        })
    })
    .unwrap_or_else(|_| Err(anyhow::anyhow!("Settings reader stopped unexpectedly")));
    Event::Loaded(Box::new(result))
}

pub fn sharing_start_work(directory: PathBuf, generation: u64) -> Event {
    let result = std::panic::catch_unwind(|| {
        SettingsFile::resolve(Some(&directory))
            .and_then(|store| SharingOwner::start_paused(store, generation))
    })
    .unwrap_or_else(|_| {
        Err(anyhow::anyhow!(
            "Sharing startup worker stopped unexpectedly"
        ))
    });
    Event::SharingStarted(result)
}

pub fn sharing_action_work(
    owner: Arc<SharingOwner>,
    action: SharingAction,
    guide: Option<Guide>,
) -> Event {
    let barrier = |receiver: crate::peer_sharing::Barrier| {
        receiver
            .recv()
            .map_err(|_| "Sharing owner stopped before acknowledging cleanup".to_owned())
            .and_then(|outcome| outcome)
    };
    let result = match action {
        SharingAction::ResumeMutation => barrier(owner.request_resume_after_mutation()),
        SharingAction::PauseForEdit => barrier(owner.request_pause(PauseReason::Mutation)),
        SharingAction::Stop => barrier(owner.request_pause(PauseReason::User)),
        SharingAction::Resume => match guide {
            Some(guide) => {
                barrier(owner.authorize_enrollment_from(&guide, &Enrollment::default(), None))
                    .and_then(|()| barrier(owner.request_resume()))
            }
            None => barrier(owner.request_resume()),
        },
    };
    Event::SharingSettled { action, result }
}

pub fn telemetry_save_work(directory: PathBuf, explicit: Option<PathBuf>, config: Config) -> Event {
    let result = std::panic::catch_unwind(|| -> Result<Config> {
        ensure!(
            crate::profiles::settings_directory(explicit.as_deref())? == directory,
            "Settings root changed"
        );
        config.save(&directory)?;
        let mut saved = Config::load_from_settings_directory(explicit.as_deref())?;
        saved.session_id = config.session_id.clone();
        let mut expected = config;
        expected.is_first_run = false;
        ensure!(
            serde_json::to_value(&saved)? == serde_json::to_value(&expected)?,
            "Telemetry configuration changed during save verification"
        );
        Ok(saved)
    })
    .unwrap_or_else(|_| {
        Err(anyhow::anyhow!(
            "Telemetry preference writer stopped unexpectedly"
        ))
    });
    Event::TelemetrySaved(Box::new(result))
}

pub fn key_work(explicit: Option<PathBuf>, key: String) -> Event {
    let result = std::panic::catch_unwind(|| -> Result<()> {
        let cancelled = AtomicBool::new(false);
        Store::from_settings_directory(explicit.as_deref())?
            .save_validated_nexus_api_key(&key, &cancelled)?;
        Ok(())
    })
    .unwrap_or_else(|_| Err(anyhow::anyhow!("Key validation stopped unexpectedly")))
    .map_err(|error| format!("{error:#}"));
    Event::KeySaved(result)
}

pub struct Consent {
    pub draft: Config,
    pub saving: bool,
}

pub struct Desktop {
    pub explicit: Option<PathBuf>,
    pub directory: PathBuf,
    pub probe: bool,
    pub settings: DesktopSettings,
    pub patcher: PatcherPreferences,
    pub telemetry: Config,
    pub consent: Option<Consent>,
    pub loaded: bool,
    pub status: String,
    pub profiles: Vec<String>,
    pub active_profile: String,
    pub peer: PeerSettings,
    pub sharing_available: bool,
    pub sharing_ready: bool,
    pub sharing_busy: bool,
    pub sharing_status: String,
    pub generation: u64,
    pub key_busy: bool,
}

impl Desktop {
    pub fn new(explicit: Option<PathBuf>, probe: bool) -> Self {
        let directory = crate::profiles::settings_directory(explicit.as_deref())
            .unwrap_or_else(|_| explicit.clone().unwrap_or_default());
        Self {
            explicit,
            directory,
            probe,
            settings: DesktopSettings::default(),
            patcher: PatcherPreferences {
                engine: PatcherEngine::default(),
                executable: None,
            },
            telemetry: Config::default(),
            consent: None,
            loaded: false,
            status: String::new(),
            profiles: Vec::new(),
            active_profile: String::new(),
            peer: PeerSettings::default(),
            sharing_available: false,
            sharing_ready: true,
            sharing_busy: false,
            sharing_status: String::new(),
            generation: 0,
            key_busy: false,
        }
    }

    pub fn store(&self) -> Result<Store> {
        Store::from_settings_directory(self.explicit.as_deref())
    }

    pub fn gate(&self) -> Result<()> {
        if self.sharing_available && (!self.sharing_ready || self.sharing_busy) {
            anyhow::bail!(SHARING_GATE);
        }
        Ok(())
    }

    pub fn owner() -> Option<Arc<SharingOwner>> {
        SHARING.lock().ok().and_then(|slot| slot.clone())
    }

    pub fn set_owner(owner: Option<SharingOwner>) {
        if let Ok(mut slot) = SHARING.lock() {
            *slot = owner.map(Arc::new);
        }
    }

    pub fn start_lifecycle(&mut self, total: usize, selected: usize) -> Result<()> {
        let mut config = self.telemetry.clone();
        if self.consent.is_some() {
            config.set_consent(false);
        }
        let lifecycle = Lifecycle::start(
            self.directory.clone(),
            crate::logging::Options {
                debug_logging: self.settings.debug_logging,
                ..Default::default()
            },
            config,
            total,
            selected,
        )?;
        if let Ok(mut slot) = LIFECYCLE.lock() {
            *slot = Some(Arc::new(lifecycle));
        }
        Ok(())
    }

    pub fn configure_lifecycle(&self, config: Config) -> Result<()> {
        let lifecycle = LIFECYCLE
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .context("Telemetry owner unavailable")?;
        lifecycle.configure(config).map(|_| ())
    }

    pub fn apply_loaded(&mut self, loaded: Loaded) -> Vec<String> {
        self.settings = loaded.settings;
        self.patcher = loaded.patcher;
        self.telemetry = if self.probe {
            Config::default()
        } else {
            loaded.config
        };
        self.profiles = loaded.profiles;
        self.active_profile = loaded.active.unwrap_or_default();
        self.peer = loaded.peer;
        self.loaded = true;
        if !self.probe && self.telemetry.is_first_run {
            self.consent = Some(Consent {
                draft: self.telemetry.clone(),
                saving: false,
            });
            self.telemetry.set_consent(false);
        }
        loaded.warnings
    }

    pub fn theme_name(&self) -> &'static str {
        match self.settings.theme.as_deref().map(str::to_ascii_lowercase) {
            None => "dark",
            Some(path) if path.contains("lightstyle") => "light",
            Some(path) if path.contains("kotor2") => "k2",
            Some(path) if path.contains("kotor") => "k1",
            Some(path) if path.contains("darkstyle") => "dark",
            Some(_) => "light",
        }
    }

    pub fn theme_path(theme: &str) -> Option<&'static str> {
        match theme {
            "dark" => Some("/Styles/DarkStyle.axaml"),
            "light" => Some("/Styles/LightStyle.axaml"),
            "k1" => Some("/Styles/KotorStyle.axaml"),
            "k2" => Some("/Styles/Kotor2Style.axaml"),
            _ => None,
        }
    }

    pub fn save_theme(&mut self, theme: &str) -> Result<()> {
        let path = Self::theme_path(theme).context("Unknown theme")?;
        self.store()?.save_theme_preference(path)?;
        self.settings.theme = Some(path.into());
        Ok(())
    }

    pub fn save_settings(&mut self, edit: &Value) -> Result<()> {
        self.gate()?;
        let mut next = self.settings.clone();
        let flag = |name: &str| edit.get(name).and_then(Value::as_bool);
        if let Some(value) = flag("spoilerFree") {
            next.spoiler_free_mode = value;
        }
        if let Some(value) = flag("debugLogging") {
            next.debug_logging = value;
        }
        if let Some(value) = flag("fileWatcher") {
            next.enable_file_watcher = value;
        }
        if let Some(value) = flag("caseInsensitive") {
            next.case_insensitive_pathing = value;
        }
        if let Some(value) = flag("attemptFixes") {
            next.attempt_fixes = value;
        }
        if let Some(value) = flag("noAdmin") {
            next.no_admin = value;
        }
        if let Some(value) = flag("archiveDeepCheck") {
            next.archive_deep_check = value;
        }
        if let Some(value) = flag("multiThreadedIo") {
            next.use_multi_threaded_io = value;
        }
        if let Some(value) = flag("copyForMove") {
            next.use_copy_for_move_actions = value;
        }
        if let Some(value) = flag("validateArchives") {
            next.validate_and_replace_invalid_archives = value;
        }
        if let Some(value) = flag("filterByResolution") {
            next.filter_downloads_by_resolution = value;
        }
        if let Some(value) = flag("managed") {
            next.managed_deployment_enabled = value;
        }
        let store = self.store()?;
        let (saved, patcher) = store.save_desktop_settings_preserving_key_with_patcher(&next)?;
        self.settings = saved;
        self.patcher = patcher;
        if let Some(engine) = edit.get("patcherEngine").and_then(Value::as_str) {
            let executable = edit
                .get("patcherPath")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let engine = match engine.to_ascii_lowercase().as_str() {
                "holopatcher" => PatcherEngine::HoloPatcher,
                "kpatcher" => PatcherEngine::KPatcher,
                "odypatcher" => PatcherEngine::OdyPatcher,
                _ => anyhow::bail!("Unsupported patcher engine"),
            };
            store.save_patcher_preferences(engine, executable)?;
            self.patcher = store.patcher_preferences()?;
        }
        crate::logging::set_debug_logging(self.settings.debug_logging);
        self.status = "Desktop preferences saved.".into();
        Ok(())
    }

    pub fn telemetry_edit(&self, edit: &Value) -> Config {
        let mut config = self.telemetry.clone();
        let flag = |name: &str| edit.get(name).and_then(Value::as_bool);
        if let Some(value) = flag("usage") {
            config.collect_usage_data = value;
        }
        if let Some(value) = flag("performance") {
            config.collect_performance_metrics = value;
        }
        if let Some(value) = flag("crash") {
            config.collect_crash_reports = value;
        }
        if let Some(value) = flag("enabled") {
            config.set_consent(value);
        }
        config.is_first_run = false;
        config
    }

    pub fn refresh_profiles(&mut self) -> Result<()> {
        let store = self.store()?;
        self.profiles = store
            .list()?
            .into_iter()
            .map(|profile| profile.name)
            .collect();
        self.active_profile = store.active_name()?.unwrap_or_default();
        Ok(())
    }

    pub fn profile_save(
        &mut self,
        name: &str,
        guide: &Guide,
        input: &str,
        game: &str,
        mods: &str,
    ) -> Result<()> {
        self.gate()?;
        let name = crate::profiles::sanitize(name)?;
        self.store()?.capture(
            &name,
            guide,
            (!input.is_empty()).then(|| Path::new(input)),
            (!game.is_empty()).then(|| Path::new(game)),
            (!mods.is_empty()).then(|| Path::new(mods)),
        )?;
        self.refresh_profiles()?;
        self.status = format!("Profile {name} saved.");
        Ok(())
    }

    pub fn profile_activate(&mut self, name: &str) -> Result<Profile> {
        self.gate()?;
        let profile = self.store()?.activate(name)?;
        self.refresh_profiles()?;
        self.status = format!("Profile {} activated.", profile.name);
        Ok(profile)
    }

    pub fn profile_clone(&mut self, from: &str, to: &str) -> Result<String> {
        self.gate()?;
        let created = self.store()?.clone_profile(from, to)?;
        self.refresh_profiles()?;
        Ok(created.name)
    }

    pub fn profile_rename(&mut self, from: &str, to: &str) -> Result<String> {
        self.gate()?;
        let renamed = self.store()?.rename(from, to)?;
        self.refresh_profiles()?;
        Ok(renamed.name)
    }

    pub fn profile_delete(&mut self, name: &str) -> Result<()> {
        self.gate()?;
        self.store()?.delete_and_deactivate(name)?;
        self.refresh_profiles()
    }

    pub fn peer_change(&mut self, key: &str, value: &Value) -> Result<()> {
        let change = match key {
            "sharing" => ("peerSharing", value.clone()),
            "keepSharing" => ("peerKeepSharing", value.clone()),
            "mode" => ("peerMode", value.clone()),
            "uploadKiBps" => ("peerUploadKiBps", value.clone()),
            _ => anyhow::bail!("Unsupported peer setting"),
        };
        let file = SettingsFile::resolve(self.explicit.as_deref())?;
        file.update_peer_settings(&[(change.0.to_owned(), change.1)])?;
        self.peer = PeerSettings::from_settings(&file.load()?);
        self.status = "Peer settings saved. Changes apply to the next run.".into();
        Ok(())
    }

    pub fn json(&self) -> Value {
        let owner = Self::owner();
        let snapshot = owner.as_ref().map(|owner| owner.snapshot());
        let telemetry = &self.telemetry;
        let consent = self.consent.as_ref().map(|consent| {
            let mut draft = consent.draft.clone();
            draft.enabled = true;
            draft.user_consented = true;
            json!({
                "usage": consent.draft.collect_usage_data,
                "performance": consent.draft.collect_performance_metrics,
                "crash": consent.draft.collect_crash_reports,
                "summary": draft.privacy_summary(),
                "saving": consent.saving,
            })
        });
        json!({
            "loaded": self.loaded,
            "status": self.status,
            "directory": self.directory.display().to_string(),
            "probe": self.probe,
            "consent": consent,
            "telemetry": {
                "enabled": telemetry.enabled && telemetry.user_consented,
                "usage": telemetry.collect_usage_data,
                "performance": telemetry.collect_performance_metrics,
                "crash": telemetry.collect_crash_reports,
                "summary": telemetry.privacy_summary(),
            },
            "settings": {
                "theme": self.theme_name(),
                "spoilerFree": self.settings.spoiler_free_mode,
                "debugLogging": self.settings.debug_logging,
                "fileWatcher": self.settings.enable_file_watcher,
                "caseInsensitive": self.settings.case_insensitive_pathing,
                "attemptFixes": self.settings.attempt_fixes,
                "noAdmin": self.settings.no_admin,
                "archiveDeepCheck": self.settings.archive_deep_check,
                "multiThreadedIo": self.settings.use_multi_threaded_io,
                "copyForMove": self.settings.use_copy_for_move_actions,
                "validateArchives": self.settings.validate_and_replace_invalid_archives,
                "filterByResolution": self.settings.filter_downloads_by_resolution,
                "managed": self.settings.managed_deployment_enabled,
                "hasNexusKey": self.settings.nexus_mods_api_key.as_deref().is_some_and(|key| !key.is_empty()),
                "keyBusy": self.key_busy,
            },
            "patcher": {
                "engine": format!("{:?}", self.patcher.engine),
                "path": self.patcher.executable.clone().unwrap_or_default(),
            },
            "profiles": {"names": self.profiles, "active": self.active_profile},
            "peer": {
                "sharing": self.peer.sharing,
                "keepSharing": self.peer.keep_sharing,
                "mode": match self.peer.mode {
                    PeerMode::Off => "off",
                    PeerMode::Race => "race",
                    PeerMode::Fallback => "fallback",
                },
                "uploadKiBps": self.peer.upload_kibps,
            },
            "sharing": {
                "available": self.sharing_available,
                "gated": self.sharing_available && !self.sharing_ready,
                "busy": self.sharing_busy,
                "status": self.sharing_status,
                "state": snapshot.as_ref().map(|snapshot| format!("{:?}", snapshot.state)),
                "uploadedBytes": snapshot.as_ref().map_or(0, |snapshot| snapshot.uploaded_bytes),
                "reason": snapshot.as_ref().and_then(|snapshot| snapshot.reason.clone()),
            },
        })
    }
}
