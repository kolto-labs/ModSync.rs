use anyhow::{Context, Result, ensure};
use clap::{Args, ValueEnum};
use modsync::{
    Guide,
    profiles::{self, SettingsFile, Store},
    recovery,
};
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone, Copy, ValueEnum)]
enum Action {
    List,
    Show,
    Create,
    Clone,
    Rename,
    Delete,
    Activate,
    Capture,
    Apply,
}
#[derive(Args)]
pub struct ProfileArgs {
    #[arg(
        long,
        short = 'a',
        value_enum,
        ignore_case = true,
        default_value = "list"
    )]
    action: Action,
    #[arg(long)]
    settings_dir: Option<PathBuf>,
    #[arg(long, short = 'n')]
    name: Option<String>,
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    to: Option<String>,
    #[arg(long)]
    input: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    game_dir: Option<PathBuf>,
    #[arg(long)]
    source_dir: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}
impl ProfileArgs {
    pub fn run(self) -> Result<()> {
        let store = Store::from_settings_directory(self.settings_dir.as_deref())?;
        let mut active_name = None;
        let required = |value: Option<&str>, flag: &str| -> Result<String> {
            let v = value
                .context(format!("Profile action requires --{flag}"))?
                .trim();
            ensure!(!v.is_empty(), "Profile action requires --{flag}");
            Ok(v.into())
        };
        let result = match self.action {
            Action::List => {
                // The reference lists profiles even when settings.json is unreadable.
                let active = store.active_name().unwrap_or_else(|e| {
                    eprintln!("Warning: failed to read settings: {e:#}");
                    None
                });
                active_name = active.clone();
                json!(store.list()?.iter().map(|p|json!({"Name":p.name,"KotorDirectory":p.kotor_directory,"ModDirectory":p.mod_directory,"InstructionFilePath":p.instruction_file_path,"ComponentSelectionCount":p.component_selections.len(),"CreatedUtc":p.created_utc,"LastUsedUtc":p.last_used_utc,"IsActive":active.as_ref().is_some_and(|s|s.eq_ignore_ascii_case(&p.name))})).collect::<Vec<_>>())
            }
            Action::Show => store
                .require(&required(self.name.as_deref(), "name")?)?
                .cli_value(),
            Action::Create => store
                .create(&required(self.name.as_deref(), "name")?)?
                .cli_value(),
            Action::Clone => store
                .clone_profile(
                    &required(self.from.as_deref().or(self.name.as_deref()), "from")?,
                    &required(self.to.as_deref(), "to")?,
                )?
                .cli_value(),
            Action::Rename => store
                .rename(
                    &required(self.from.as_deref().or(self.name.as_deref()), "from")?,
                    &required(self.to.as_deref(), "to")?,
                )?
                .cli_value(),
            Action::Delete => {
                let name = required(self.name.as_deref(), "name")?;
                ensure!(store.delete(&name)?, "Profile '{name}' was not found");
                json!({"Deleted":name})
            }
            Action::Activate => store
                .activate(&required(self.name.as_deref(), "name")?)?
                .cli_value(),
            Action::Capture => {
                let input = self.input.as_ref().context("Capture requires --input")?;
                let guide = Guide::load_preserving_selection(input)?;
                store
                    .capture(
                        &required(self.name.as_deref(), "name")?,
                        &guide,
                        Some(input),
                        self.game_dir.as_deref(),
                        self.source_dir.as_deref(),
                    )?
                    .cli_value()
            }
            Action::Apply => {
                let input = self.input.as_ref().context("Apply requires --input")?;
                let output = self.output.as_ref().context("Apply requires --output")?;
                let name = required(self.name.as_deref(), "name")?;
                let mut guide = Guide::load_preserving_selection(input)?;
                let profile = store.apply(&name, &mut guide)?;
                let format = output
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("toml");
                let text = guide.render(format)?;
                recovery::atomic_write(&std::path::absolute(output)?, text.as_bytes())?;
                profile.cli_value()
            }
        };
        if self.json {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            let name = result["Name"].as_str().unwrap_or_default();
            match self.action {
                Action::List => {
                    let profiles = result.as_array().context("Profile listing shape differs")?;
                    if profiles.is_empty() {
                        println!("No profiles found in {}", store.directory().display());
                    } else {
                        println!("Profiles ({}):", profiles.len());
                        for profile in profiles {
                            println!(
                                "  - {}{}",
                                profile["Name"].as_str().unwrap_or_default(),
                                if profile["IsActive"].as_bool() == Some(true) {
                                    " *"
                                } else {
                                    ""
                                }
                            );
                        }
                        if let Some(active) = active_name.filter(|value| !value.trim().is_empty()) {
                            println!("Active profile (settings.json): {active}");
                        }
                    }
                }
                Action::Show => {
                    println!("Profile: {name}");
                    for (label, key) in [
                        ("KOTOR", "KotorDirectory"),
                        ("Mod workspace", "ModDirectory"),
                        ("Instruction file", "InstructionFilePath"),
                    ] {
                        println!("  {label}: {}", result[key].as_str().unwrap_or("(unset)"));
                    }
                    println!(
                        "  Component selections: {}",
                        result["ComponentSelections"]
                            .as_object()
                            .map_or(0, |values| values.len())
                    );
                }
                Action::Create => println!(
                    "Created profile '{name}' under {}",
                    store.directory().display()
                ),
                Action::Delete => println!(
                    "Deleted profile '{}'.",
                    result["Deleted"].as_str().unwrap_or_default()
                ),
                Action::Activate => {
                    println!("Activated profile '{name}' (saved to settings.json).")
                }
                Action::Clone | Action::Rename => {
                    let from = self
                        .from
                        .as_deref()
                        .or(self.name.as_deref())
                        .unwrap_or_default()
                        .trim();
                    let verb = if matches!(self.action, Action::Clone) {
                        "Cloned"
                    } else {
                        "Renamed"
                    };
                    println!("{verb} profile '{from}' to '{name}'.");
                }
                Action::Capture => println!(
                    "Captured profile '{name}' under {}",
                    store.directory().display()
                ),
                Action::Apply => println!(
                    "Applied profile '{name}' to {}",
                    self.output
                        .as_ref()
                        .context("Apply output missing")?
                        .display()
                ),
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum SettingsAction {
    /// Inspect all typed desktop settings with reference defaults applied.
    Inspect,
    List,
    Get,
    Set,
}
/// Read and write settings.json keys like the reference `settings` verb.
#[derive(Args)]
pub struct SettingsArgs {
    #[arg(long, short = 'a', value_enum, ignore_case = true)]
    action: SettingsAction,
    #[arg(long, short = 'k')]
    key: Option<String>,
    /// Omit or leave empty to remove the key. true/false, null, numbers and JSON
    /// objects or arrays are stored as JSON; anything else as text.
    #[arg(long, allow_hyphen_values = true)]
    value: Option<String>,
    #[arg(long)]
    settings_dir: Option<PathBuf>,
    #[arg(long)]
    json: bool,
    /// Show nexusModsApiKey and other secrets instead of ***.
    #[arg(long)]
    reveal_secrets: bool,
}
impl SettingsArgs {
    pub fn run(self) -> Result<()> {
        let file = SettingsFile::resolve(self.settings_dir.as_deref())?;
        let settings_file = file.path.to_string_lossy().into_owned();
        let key = || -> Result<String> {
            let k = self.key.as_deref().map(str::trim).unwrap_or_default();
            let action = match self.action {
                SettingsAction::Get => "get",
                _ => "set",
            };
            ensure!(!k.is_empty(), "Settings action '{action}' requires --key.");
            Ok(k.into())
        };
        let print = |value: &serde_json::Value| -> Result<()> {
            println!("{}", serde_json::to_string_pretty(value)?);
            Ok(())
        };
        match self.action {
            SettingsAction::Inspect => {
                let report = Store::from_settings_directory(self.settings_dir.as_deref())?
                    .desktop_settings();
                for warning in &report.warnings {
                    eprintln!("Warning: {warning}");
                }
                let values = report.settings.output_value(self.reveal_secrets)?;
                if self.json {
                    return print(
                        &json!({"settingsDirectory":file.directory,"settingsFile":settings_file,"values":values,"usedDefaults":report.used_defaults,"targetGame":report.settings.target_game(),"warnings":report.warnings}),
                    );
                }
                println!(
                    "Desktop settings{}:",
                    if report.used_defaults {
                        " (defaults after load error)"
                    } else {
                        ""
                    }
                );
                for (key, value) in values
                    .as_object()
                    .context("Typed settings must be an object")?
                {
                    println!("  {key}={}", profiles::settings_text(value));
                }
                println!(
                    "  targetGame={}",
                    report.settings.target_game().unwrap_or("(none)")
                );
                Ok(())
            }
            SettingsAction::List => {
                // Reading matches the reference: an unusable file lists as empty.
                let root = file.load().unwrap_or_else(|e| {
                    eprintln!("Warning: failed to parse settings file '{settings_file}': {e:#}");
                    json!({})
                });
                let values = profiles::redacted(&root, self.reveal_secrets);
                if self.json {
                    return print(&json!({
                        "settingsDirectory": file.directory.to_string_lossy(),
                        "settingsFile": settings_file,
                        "values": values,
                    }));
                }
                let map = values.as_object().unwrap();
                if map.is_empty() {
                    println!("No settings found in {settings_file}");
                    return Ok(());
                }
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort_by_key(|k| k.to_lowercase());
                println!("Settings ({}):", keys.len());
                for k in keys {
                    println!("  {k}={}", profiles::settings_text(&map[k.as_str()]));
                }
                Ok(())
            }
            SettingsAction::Get => {
                let key = key()?;
                let root = file.load().unwrap_or_else(|e| {
                    eprintln!("Warning: failed to parse settings file '{settings_file}': {e:#}");
                    json!({})
                });
                // Exact-case lookup, as in the reference JsonObject.
                let mut value = root
                    .get(&key)
                    .cloned()
                    .with_context(|| format!("Setting '{key}' was not found."))?;
                if !self.reveal_secrets && profiles::is_sensitive_key(&key) {
                    value = "***".into();
                }
                if self.json {
                    return print(
                        &json!({"key": key, "value": value, "settingsFile": settings_file}),
                    );
                }
                println!("{}", profiles::settings_text(&value));
                Ok(())
            }
            SettingsAction::Set => {
                let key = key()?;
                // Deliberately stricter than the reference, which replaces an unreadable
                // file with a new object: refuse instead of discarding the user's data.
                // Native locale lookup and parsing complete before taking the settings lock.
                let value = self
                    .value
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map(profiles::parse_cli_value_current_culture)
                    .transpose()?;
                let root = file
                    .update(|root| {
                        let object = root.as_object_mut().context("Settings must be an object")?;
                        if let Some(value) = value {
                            object.insert(key.clone(), value);
                        } else {
                            object.shift_remove(&key);
                        }
                        Ok(())
                    })
                    .with_context(|| {
                        format!("Refusing to overwrite unreadable settings file {settings_file}")
                    })?;
                if self.json {
                    let shown = profiles::redacted(&root, false);
                    return print(&json!({
                        "key": key,
                        "value": shown.get(&key).cloned().unwrap_or(serde_json::Value::Null),
                        "settingsFile": settings_file,
                    }));
                }
                println!("Updated setting '{key}' in {settings_file}");
                Ok(())
            }
        }
    }
}

/// Reference-compatible key command; validation is on unless explicitly skipped.
#[derive(Args)]
pub struct SetNexusApiKeyArgs {
    /// Nexus personal API key. It is never included in command output.
    api_key: String,
    #[arg(long)]
    skip_validation: bool,
    #[arg(long)]
    settings_dir: Option<PathBuf>,
    #[arg(long)]
    json: bool,
}
impl SetNexusApiKeyArgs {
    pub fn run(self) -> Result<()> {
        ensure!(
            !self.api_key.trim().is_empty(),
            "Nexus API key cannot be blank"
        );
        let store = Store::from_settings_directory(self.settings_dir.as_deref())?;
        let cancelled = modsync::p2p::stop_signal_flag()?;
        let result = if self.skip_validation {
            ensure!(
                !cancelled.load(std::sync::atomic::Ordering::Acquire),
                "Nexus key save cancelled"
            );
            store.save_nexus_api_key(&self.api_key).map(|()| None)
        } else {
            store
                .save_validated_nexus_api_key(&self.api_key, &cancelled)
                .map(Some)
        };
        let validation = match result {
            Err(_) if cancelled.load(std::sync::atomic::Ordering::Acquire) => {
                eprintln!("Nexus key validation cancelled; no successful save was reported.");
                return Err(crate::cli_session::Exit(130).into());
            }
            other => other?,
        };
        if self.json {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"saved":true,"validated":validation.is_some(),"validation":validation.as_ref().map(|value|json!({"isValid":value.is_valid,"userName":value.user_name,"isPremium":value.is_premium,"message":value.message}))})
                )?
            );
        } else {
            println!(
                "Nexus Mods API key saved{}.",
                if validation.is_some() {
                    " and validated"
                } else {
                    " without validation"
                }
            );
        }
        Ok(())
    }
}
