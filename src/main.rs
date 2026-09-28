mod profile_cli;
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use modsync::{Engine, Guide, PatcherEngine, Settings, download, paths::Roots, recovery};
use std::{
    ffi::OsString,
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[cfg(feature = "desktop")]
mod fomod_ui;
#[cfg(feature = "desktop")]
mod ui;

#[derive(Parser)]
#[command(
    version,
    about = "Install ModSync guides with a Rust desktop app or CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    #[command(external_subcommand)]
    ProtocolHandoff(Vec<OsString>),
    /// Manage non-destructive staged deployments and their manifests.
    Deployment(DeploymentArgs),
    /// Manage stored install profiles.
    Profile(profile_cli::ProfileArgs),
    /// Get, set or list settings.json values.
    Settings(profile_cli::SettingsArgs),
    /// Open the desktop installer (also the default with no command).
    Gui {
        #[arg(long)]
        settings_dir: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
        #[arg(long)]
        game_dir: Option<PathBuf>,
        #[arg(long)]
        source_dir: Option<PathBuf>,
        /// A modsync:// URL received from an OS protocol handler.
        #[arg(long)]
        protocol: Option<String>,
        /// Start a separate desktop process instead of forwarding to the running instance.
        #[arg(long)]
        allow_multiple_instances: bool,
    },
    /// Inspect FOMOD metadata and selection defaults as JSON without installing.
    FomodInspect {
        archive: PathBuf,
        /// Apply the matching archive entry from a version-1 choices JSON file.
        #[arg(long)]
        choices: Option<PathBuf>,
        /// Include mapped copy instructions for this relative mod-directory extraction path.
        #[arg(long)]
        extracted_directory: Option<String>,
        /// Save the current selections as a reusable version-1 choices file.
        #[arg(long)]
        save_choices: Option<PathBuf>,
        /// Prompt for plugin selections on stdin; prompts are written to stderr.
        #[arg(long)]
        interactive: bool,
        #[arg(long)]
        scratch_dir: Option<PathBuf>,
        #[arg(long)]
        max_expanded_bytes: Option<u64>,
    },
    /// List components and their download links as JSON.
    List { input: PathBuf },
    /// Parse an nxm:// or modsync:// handoff URL without fetching it.
    ProtocolParse { url: String },
    /// Download an HTTP(S) instruction file with the protocol fetcher's SSRF and size guards.
    ProtocolFetch { url: String, output: PathBuf },
    /// Fetch and load a modsync:// instruction link without retaining its temporary download.
    ProtocolOpen { url: String },
    /// Resolve an nxm:// handoff to exactly one top-level component in a guide.
    ProtocolResolve { input: PathBuf, url: String },
    /// Register modsync:// as a Linux desktop handler for this executable.
    ProtocolRegisterLinux {
        executable: PathBuf,
        #[arg(long)]
        applications_dir: Option<PathBuf>,
    },
    /// Resolve an nxm:// link through Nexus and download its archive.
    NxmDownload {
        url: String,
        /// The archive path; with --input, the mod workspace directory instead.
        output: PathBuf,
        /// Guide to resolve the link against. Runs the shared acquisition path
        /// (nxm_target, run, record) and prints the run report.
        #[arg(long)]
        input: Option<PathBuf>,
        /// With --input, write Files[name] = true back into the guide file.
        #[arg(long, requires = "input")]
        record: bool,
        #[arg(long)]
        nexus_mods_api_key: Option<String>,
        #[arg(long)]
        settings_dir: Option<PathBuf>,
        /// Probe-only Nexus API base URL. Requires an explicit API-key flag.
        #[arg(long, hide = true)]
        nexus_api_base: Option<String>,
    },
    /// Apply provider version data from a JSON object keyed as game/modId.
    UpdateApply {
        input: PathBuf,
        versions: PathBuf,
        output: PathBuf,
        #[arg(long)]
        summary: Option<PathBuf>,
    },
    /// Query Nexus Mods for tracked resource versions and persist the updated guide.
    UpdateCheck {
        input: PathBuf,
        output: PathBuf,
        /// Overrides nexusModsApiKey from settings.json.
        #[arg(long)]
        nexus_mods_api_key: Option<String>,
        #[arg(long)]
        settings_dir: Option<PathBuf>,
    },
    /// Merge two guides, matching GUIDs then normalized names/authors.
    Merge {
        existing: PathBuf,
        incoming: PathBuf,
        output: PathBuf,
        #[arg(long)]
        use_existing_order: bool,
        #[arg(long)]
        prefer_existing_fields: bool,
        #[arg(long)]
        prefer_existing_name: bool,
        #[arg(long)]
        prefer_existing_author: bool,
        #[arg(long)]
        prefer_existing_description: bool,
        #[arg(long)]
        prefer_existing_directions: bool,
        #[arg(long)]
        prefer_existing_category: bool,
        #[arg(long)]
        prefer_existing_tier: bool,
        #[arg(long)]
        prefer_existing_installation_method: bool,
        #[arg(long)]
        prefer_existing_instructions: bool,
        #[arg(long)]
        prefer_existing_options: bool,
        #[arg(long, alias = "prefer-existing-modlinks")]
        prefer_existing_resource_registry: bool,
        /// Accepted for C# CLI compatibility. Incoming values already win by default.
        #[arg(long)]
        prefer_incoming_fields: bool,
        #[arg(long)]
        exclude_existing_only: bool,
        #[arg(long)]
        exclude_incoming_only: bool,
    },
    /// Print the resolved selection without downloading or installing.
    Selection(SelectionArgs),
    /// Simulate instructions without changing the game or mod directory.
    Validate(InstallArgs),
    /// Report conflicting simulated game-file writes as JSON, without installing.
    Conflicts(InstallArgs),
    /// Validate, snapshot, then install in dependency order. Stop at the first failure.
    Install(InstallArgs),
    /// Restore both directories from the verified pre-install snapshot after a failure.
    Restore {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        source_dir: PathBuf,
    },
    /// Print the saved completion receipt summary, or report that there is none.
    CompletionSummary {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        source_dir: PathBuf,
        /// Print the summary as JSON, or null when there is no receipt.
        #[arg(long)]
        json: bool,
    },
    /// Convert TOML, JSON, YAML, XML, or Markdown input.
    Convert {
        input: Option<PathBuf>,
        output: Option<PathBuf>,
        #[arg(short = 'i', long = "input", conflicts_with_all = ["input", "stdin"])]
        input_path: Option<PathBuf>,
        #[arg(short = 'o', long = "output", conflicts_with = "output")]
        output_path: Option<PathBuf>,
        #[arg(long, conflicts_with = "input")]
        stdin: bool,
        /// Output format; defaults to TOML, or the positional output extension.
        #[arg(short, long)]
        format: Option<String>,
        /// Draft instructions from prose; generated steps require review before installation.
        #[arg(long)]
        parse_directions: bool,
        /// Infer missing instructions from local downloaded archives.
        #[arg(long, requires = "source_path")]
        auto_generate_local: bool,
        #[arg(long)]
        source_path: Option<PathBuf>,
        /// Mark a component's displayed drafts reviewed (GUID; repeat for multiple components).
        #[arg(long)]
        review_draft: Vec<String>,
    },
    /// Report which host-provided filenames the guide would request; no downloads.
    DownloadPlan {
        input: PathBuf,
        #[arg(long)]
        component: String,
        #[arg(long)]
        url: String,
        #[arg(long = "filename")]
        filenames: Vec<String>,
        #[arg(long)]
        source_dir: Option<PathBuf>,
    },
    /// Fetch and verify a direct HTTPS archive URL.
    Download {
        url: String,
        output: PathBuf,
        #[arg(long)]
        sha256: Option<String>,
        /// Optional expanded-byte budget for the integrity check.
        #[arg(long)]
        max_expanded_bytes: Option<u64>,
    },
    /// Fetch one content-addressed archive from a BitTorrent swarm through a SOCKS5 proxy.
    TorrentDownload {
        magnet: String,
        output: PathBuf,
        /// Required content identity; the torrent is discarded on mismatch.
        #[arg(long)]
        sha256: String,
        /// Known seeder address from torrent-seed; repeat for fallbacks. Dialed through the proxy.
        #[arg(long)]
        peer: Vec<std::net::SocketAddr>,
        /// Optional expanded-byte budget for the integrity check.
        #[arg(long)]
        max_expanded_bytes: Option<u64>,
        /// Authored info dictionary (base64), from the guide's TorrentInfo or torrent-seed's torrentInfo.
        #[arg(long)]
        torrent_info: Option<String>,
        #[command(flatten)]
        proxy: TorrentProxyArgs,
    },
    /// Validate and seed one archive until Ctrl+C; prints its magnet and SHA-256.
    TorrentSeed {
        archive: PathBuf,
        /// Optional tracker announce URL; repeat to add fallbacks. Only http(s) trackers are used through the proxy.
        #[arg(long)]
        tracker: Vec<String>,
        /// Share under this authored magnet instead of creating a new torrent.
        #[arg(long)]
        authored_magnet: Option<String>,
        /// Authored info dictionary (base64) to serve as is; needs --authored-magnet.
        #[arg(long)]
        torrent_info: Option<String>,
        #[command(flatten)]
        proxy: TorrentProxyArgs,
    },
    /// Download a JSON array of direct HTTPS jobs with bounded concurrency and retries.
    DownloadBatch {
        input: PathBuf,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value_t = 1)]
        retries: usize,
        #[arg(long)]
        scratch_dir: Option<PathBuf>,
        #[arg(long)]
        max_expanded_bytes: Option<u64>,
    },
    /// Report the provider selected for an HTTP(S) URL without downloading it.
    DownloadProvider { url: String },
    /// Plan and run the selected-guide acquisition for every selected component with
    /// resources: existing/Origin/Peer sources, optional recording and FOMOD follow-ups.
    FetchDownloads {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        source_dir: PathBuf,
        /// Detect the game here to apply Aspyr-only rules. Without it they are only reported.
        #[arg(long)]
        game_dir: Option<PathBuf>,
        /// Honor IsSelected flags; by default select all top-level components.
        #[arg(long)]
        use_file_selection: bool,
        /// Select component names/GUIDs or category:Name/tier:Name filters (repeat the flag).
        #[arg(long)]
        select: Vec<String>,
        /// Select option GUIDs and their parent mods (repeat the flag).
        #[arg(long)]
        option: Vec<String>,
        #[command(flatten)]
        selection_flags: SelectionFlags,
        #[arg(long, default_value_t = 4)]
        workers: usize,
        #[arg(long, default_value_t = 1)]
        retries: usize,
        #[arg(long)]
        nexus_mods_api_key: Option<String>,
        #[arg(long)]
        settings_dir: Option<PathBuf>,
        /// Probe-only API base override (HTTPS, or loopback HTTP); requires an explicit key.
        #[arg(long, hide = true)]
        nexus_api_base: Option<String>,
        /// Per-invocation CA for isolated loopback HTTPS archive probes.
        #[arg(long, hide = true)]
        extra_ca_file: Option<PathBuf>,
        /// Overrides the stored peerMode; peerSharing=false always wins regardless.
        #[arg(long, value_enum)]
        peer_mode: Option<PeerModeArg>,
        /// Existing scratch folder outside the game and mod trees (defaults to system temp).
        #[arg(long)]
        scratch_dir: Option<PathBuf>,
        /// Optional expanded-byte budget per archive; extraction also checks free scratch space.
        #[arg(long)]
        max_expanded_bytes: Option<u64>,
        /// Print the effective plan and exit without downloading, recording or saving.
        #[arg(long)]
        dry_run: bool,
        /// Emit newline-delimited JSON acquisition events, then one summary line.
        #[arg(long)]
        progress_json: bool,
        /// Update the guide's selected/already-present resource facts on success.
        #[arg(long)]
        record: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum PeerModeArg {
    Off,
    Race,
    Fallback,
}
impl From<PeerModeArg> for modsync::p2p::PeerMode {
    fn from(value: PeerModeArg) -> Self {
        match value {
            PeerModeArg::Off => Self::Off,
            PeerModeArg::Race => Self::Race,
            PeerModeArg::Fallback => Self::Fallback,
        }
    }
}
#[derive(Args)]
struct DeploymentArgs {
    #[command(subcommand)]
    action: DeploymentAction,
}
/// Deployment roots: explicit staging and manifest directories, or a profile's
/// `profiles/<name>/{staging,deployment}` (the active profile when none is named).
#[derive(Args)]
struct DeploymentRoots {
    #[arg(long)]
    game_dir: PathBuf,
    #[arg(long)]
    staging_dir: Option<PathBuf>,
    #[arg(long)]
    manifest_dir: Option<PathBuf>,
    /// Profile whose staging and manifest roots to use.
    #[arg(long)]
    profile: Option<String>,
    /// Settings directory holding profiles/ and settings.json.
    #[arg(long)]
    settings_dir: Option<PathBuf>,
}
impl DeploymentRoots {
    fn service(&self) -> Result<modsync::deployment::Service> {
        modsync::deployment::Service::resolve(
            &self.game_dir,
            self.staging_dir.as_deref(),
            self.manifest_dir.as_deref(),
            self.settings_dir.as_deref(),
            self.profile.as_deref(),
        )
    }
}
#[derive(Subcommand)]
enum DeploymentAction {
    /// List deployed component manifests, oldest first.
    Status {
        #[command(flatten)]
        roots: DeploymentRoots,
    },
    /// Deploy staging-dir/<component> into the game directory.
    Deploy {
        #[command(flatten)]
        roots: DeploymentRoots,
        #[arg(long)]
        component: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove files owned by one manifest and restore its displaced files.
    Uninstall {
        #[command(flatten)]
        roots: DeploymentRoots,
        #[arg(long)]
        component: String,
    },
    /// Uninstall all manifests newest first.
    Purge {
        #[command(flatten)]
        roots: DeploymentRoots,
    },
    /// Write a hash index of the game directory (run before a patcher).
    Snapshot {
        #[command(flatten)]
        roots: DeploymentRoots,
        #[arg(long)]
        out: PathBuf,
    },
    /// Record game files changed since a snapshot into a component manifest.
    Record {
        #[command(flatten)]
        roots: DeploymentRoots,
        #[arg(long)]
        component: String,
        #[arg(long)]
        name: Option<String>,
        /// Hash index written by `deployment snapshot`.
        #[arg(long)]
        before: PathBuf,
        /// Tree mirroring the game directory before the patcher ran; supplies backups for changed files.
        #[arg(long)]
        backup_source: Option<PathBuf>,
    },
    /// Activate another profile. Refused while the current profile has deployments unless --purge-current removes them first.
    Switch {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        settings_dir: Option<PathBuf>,
        /// Profile to activate.
        #[arg(long)]
        to: String,
        /// Purge the active profile's deployments before switching.
        #[arg(long)]
        purge_current: bool,
    },
}
#[derive(Args)]
struct InstallArgs {
    /// Run all selected components, the base, or the verified remaining suffix.
    #[arg(long, value_enum, default_value = "all")]
    phase: PhaseArg,
    /// Install a Markdown guide in source order after direct-Markdown preflight.
    #[arg(long, conflicts_with_all = ["fomod_choices", "profile", "active_profile", "use_file_selection", "select", "option", "deselect", "deselect_option", "select_through_tier", "add_category", "select_visible_search", "spoiler_free"])]
    direct_markdown: bool,
    /// Version-1 FOMOD choices JSON; overrides embedded choices for this run.
    #[arg(long)]
    fomod_choices: Option<PathBuf>,
    #[arg(long, short, required_unless_present_any = ["profile", "active_profile"])]
    input: Option<PathBuf>,
    #[arg(long, required_unless_present_any = ["profile", "active_profile"])]
    game_dir: Option<PathBuf>,
    #[arg(long, required_unless_present_any = ["profile", "active_profile"])]
    source_dir: Option<PathBuf>,
    /// Apply this stored profile before explicit selection overrides.
    #[arg(long, conflicts_with = "active_profile")]
    profile: Option<String>,
    /// Use activeProfileName from the specified settings directory.
    #[arg(long)]
    active_profile: bool,
    #[arg(long)]
    settings_dir: Option<PathBuf>,
    /// Honor IsSelected flags; by default select all top-level components.
    #[arg(long)]
    use_file_selection: bool,
    /// Select component names/GUIDs or category:Name/tier:Name filters (repeat the flag).
    #[arg(long)]
    select: Vec<String>,
    /// Select option GUIDs and their parent mods (repeat the flag). Existing option selections are retained.
    #[arg(long)]
    option: Vec<String>,
    #[command(flatten)]
    selection_flags: SelectionFlags,
    /// Trust this guide to run programs with your user permissions.
    #[arg(long)]
    allow_processes: bool,
    /// Patcher executable for Patcher instructions.
    #[arg(
        long,
        visible_alias = "kpatcher-path",
        visible_alias = "odypatcher-path"
    )]
    patcher: Option<PathBuf>,
    /// CLI patcher backend for Patcher instructions.
    #[arg(long, value_enum)]
    patcher_engine: Option<PatcherEngineArg>,
    /// Existing scratch folder outside the game and mod trees (defaults to system temp).
    #[arg(long)]
    scratch_dir: Option<PathBuf>,
    /// Optional expanded-byte budget per archive; extraction also checks free scratch space.
    #[arg(long)]
    max_expanded_bytes: Option<u64>,
    /// Emit newline-delimited JSON events.
    #[arg(long)]
    json: bool,
    /// Emit newline-delimited typed installation progress on stdout.
    ///
    /// Each line is {"type":"installProgress","progress":{...}}. Success is exit
    /// code 0 with a last line whose phase is "completed". Failure or
    /// cancellation is a nonzero exit, no "completed" line, and the error on
    /// stderr. A failure before installation starts prints no lines. Legacy
    /// events and external program output go to stderr in this mode.
    #[arg(long, conflicts_with_all = ["json", "validation_report"])]
    install_progress_json: bool,
    /// Emit the structured validation pipeline report instead of validation events.
    #[arg(long, conflicts_with = "json")]
    validation_report: bool,
    /// Include a non-mutating engine dry-run result in the validation report.
    #[arg(long, requires = "validation_report")]
    dry_run: bool,
    /// Include environment, conflict, and install-order validation stages.
    #[arg(long, requires = "validation_report")]
    full: bool,
    /// Run only FOMOD configuration and VFS dry-run validation.
    #[arg(long, requires = "validation_report")]
    dry_run_only: bool,
}

/// Selection changes applied after --select, in command-line order together with --option.
#[derive(Args)]
struct SelectionFlags {
    /// Deselect a mod by name or GUID; mods that depend on it are deselected too (repeat the flag).
    #[arg(long, value_name = "NAME|GUID")]
    deselect: Vec<String>,
    /// Deselect an option GUID; whatever depends on it is deselected too (repeat the flag).
    #[arg(long, value_name = "GUID")]
    deselect_option: Vec<String>,
    /// Add base mods in this tier and every higher tier (repeat the flag).
    #[arg(long, value_name = "TIER")]
    select_through_tier: Vec<String>,
    /// Add base mods in this category, ignoring case (repeat the flag).
    #[arg(long, value_name = "CATEGORY")]
    add_category: Vec<String>,
    /// Add base mods whose shown name or description contains this text (repeat the flag).
    #[arg(long, value_name = "TEXT")]
    select_visible_search: Vec<String>,
    /// Match --select-visible-search against spoiler-free names and descriptions.
    #[arg(long)]
    spoiler_free: bool,
}

#[derive(Args)]
struct SelectionArgs {
    #[arg(long, short)]
    input: PathBuf,
    /// Detect the game here to apply Aspyr-only rules. Without it they are only reported.
    #[arg(long)]
    game_dir: Option<PathBuf>,
    /// Honor IsSelected flags; by default select all top-level components.
    #[arg(long)]
    use_file_selection: bool,
    /// Select component names/GUIDs or category:Name/tier:Name filters (repeat the flag).
    #[arg(long)]
    select: Vec<String>,
    /// Select option GUIDs and their parent mods (repeat the flag).
    #[arg(long)]
    option: Vec<String>,
    #[command(flatten)]
    selection_flags: SelectionFlags,
    /// Print JSON instead of text.
    #[arg(long)]
    json: bool,
}

/// --option and the SelectionFlags as shared actions, in command-line order.
fn ordered_selection_steps(
    option: &[String],
    flags: &SelectionFlags,
) -> Vec<modsync::selection::Action> {
    use modsync::selection::{Action as Step, Filter};
    let matches = <Cli as clap::CommandFactory>::command()
        .try_get_matches_from(std::env::args_os())
        .ok();
    let sub = matches
        .as_ref()
        .and_then(|m| m.subcommand())
        .map(|(_, s)| s);
    let mut steps = Vec::new();
    let mut push = |id: &str, values: &[String], make: &dyn Fn(&str) -> Step| {
        let indices: Vec<usize> = sub
            .and_then(|s| s.indices_of(id))
            .map(Iterator::collect)
            .unwrap_or_default();
        for (position, value) in values.iter().enumerate() {
            let order = indices.get(position).copied().unwrap_or(usize::MAX);
            steps.push((order, make(value)));
        }
    };
    push("option", option, &|guid| Step::SetOption {
        guid: guid.into(),
        selected: true,
    });
    push("deselect", &flags.deselect, &|guid| Step::SetComponent {
        guid: guid.into(),
        selected: false,
    });
    push("deselect_option", &flags.deselect_option, &|guid| {
        Step::SetOption {
            guid: guid.into(),
            selected: false,
        }
    });
    push("select_through_tier", &flags.select_through_tier, &|tier| {
        Step::SelectThroughTier(tier.into())
    });
    push("add_category", &flags.add_category, &|category| {
        Step::SelectCategories(vec![category.into()])
    });
    push(
        "select_visible_search",
        &flags.select_visible_search,
        &|search| {
            Step::SelectVisible(Filter {
                search: search.into(),
                spoiler_free: flags.spoiler_free,
                ..Filter::default()
            })
        },
    );
    steps.sort_by_key(|(order, _)| *order);
    steps.into_iter().map(|(_, step)| step).collect()
}

/// Explicit selection flags go through the shared actions; default-all, file and
/// profile selections are left as loaded. Game rules run once at the end.
/// Warnings and every resolution change go to stderr when `report` is set.
fn apply_selection(
    components: &mut [modsync::Component],
    game: Option<modsync::game::Detected>,
    select: &[String],
    option: &[String],
    flags: &SelectionFlags,
    report: bool,
) -> Result<Vec<modsync::selection::Outcome>> {
    use modsync::selection::{Action as Step, apply, describe};
    let mut steps = Vec::new();
    if !select.is_empty() {
        steps.push(Step::ResetFromSelectors(select.to_vec()));
    }
    steps.extend(ordered_selection_steps(option, flags));
    steps.push(Step::EnforceGame);
    let mut outcomes = Vec::new();
    for step in steps {
        let outcome = apply(components, game, step)?;
        if report {
            for warning in &outcome.warnings {
                eprintln!("Selection warning: {warning}");
            }
            for guid in &outcome.auto_selected {
                eprintln!("Selection: auto-selected {}", describe(components, guid));
            }
            for guid in &outcome.auto_deselected {
                eprintln!("Selection: auto-deselected {}", describe(components, guid));
            }
        }
        outcomes.push(outcome);
    }
    Ok(outcomes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum PhaseArg {
    All,
    Base,
    Remaining,
}
impl From<PhaseArg> for modsync::phase::Phase {
    fn from(value: PhaseArg) -> Self {
        match value {
            PhaseArg::All => Self::All,
            PhaseArg::Base => Self::Base,
            PhaseArg::Remaining => Self::Remaining,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PatcherEngineArg {
    #[value(name = "ody-patcher", alias = "OdyPatcher", alias = "odypatcher")]
    Ody,
    #[value(name = "k-patcher", alias = "KPatcher", alias = "kpatcher")]
    K,
    #[value(name = "holo-patcher", alias = "Holopatcher", alias = "holopatcher")]
    Holo,
}
impl From<PatcherEngineArg> for PatcherEngine {
    fn from(value: PatcherEngineArg) -> Self {
        match value {
            PatcherEngineArg::Ody => Self::OdyPatcher,
            PatcherEngineArg::K => Self::KPatcher,
            PatcherEngineArg::Holo => Self::HoloPatcher,
        }
    }
}

/// Privacy options shared by the torrent verbs.
#[derive(Args, Debug)]
struct TorrentProxyArgs {
    /// SOCKS5 proxy for every peer and tracker connection (socks5://IP:PORT); repeat to add fallbacks.
    #[arg(long)]
    peer_proxy: Vec<String>,
    /// Public SOCKS5 list tried when no --peer-proxy works. Default: the monosans list. Pass "" to disable.
    #[arg(long)]
    peer_proxy_list_url: Option<String>,
    /// HTTP(S) tracker list URL; repeat to add more. Default: the ngosang HTTP and HTTPS lists.
    #[arg(long)]
    tracker_list_url: Vec<String>,
    /// Connect to peers directly, with DHT, a listener and UPnP. Peers will see your IP address.
    #[arg(long)]
    no_proxy: bool,
    /// Upload cap in KiB/s (default 100, minimum 16).
    #[arg(long)]
    upload_kibps: Option<u32>,
    /// Download from peers without uploading to them.
    #[arg(long)]
    no_upload_while_downloading: bool,
    /// With --authored-magnet: stop sharing after this many hours (default 24; 0 = no limit).
    #[arg(long)]
    share_hours: Option<f64>,
    /// With --authored-magnet: stop sharing at this upload ratio (default 1.0; 0 = no limit).
    #[arg(long)]
    share_ratio: Option<f64>,
}

impl From<TorrentProxyArgs> for modsync::p2p::CliProxyOptions {
    fn from(args: TorrentProxyArgs) -> Self {
        Self {
            no_proxy: args.no_proxy,
            proxies: args.peer_proxy,
            proxy_list_url: args.peer_proxy_list_url,
            tracker_list_urls: args.tracker_list_url,
            upload_kibps: args.upload_kibps,
            no_upload_while_downloading: args.no_upload_while_downloading,
            share_hours: args.share_hours,
            share_ratio: args.share_ratio,
        }
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Some(Action::ProtocolHandoff(arguments)) => {
            ensure!(
                arguments.len() == 1,
                "Protocol handoff accepts exactly one URL"
            );
            let raw = arguments[0]
                .to_str()
                .context("Protocol URL must be valid UTF-8")?;
            println!(
                "{}",
                serde_json::to_string_pretty(&modsync::protocol::parse(raw)?)?
            );
            Ok(())
        }
        None => gui(None, None, None, None, None, false),
        Some(Action::Gui {
            settings_dir,
            input,
            game_dir,
            source_dir,
            protocol,
            allow_multiple_instances,
        }) => gui(
            input,
            game_dir,
            source_dir,
            settings_dir,
            protocol,
            allow_multiple_instances,
        ),
        Some(Action::Deployment(args)) => {
            let print = |value: serde_json::Value| -> Result<()> {
                println!("{}", serde_json::to_string_pretty(&value)?);
                Ok(())
            };
            let warn = |warnings: &[String]| {
                for warning in warnings {
                    eprintln!("warning: {warning}");
                }
            };
            match args.action {
                DeploymentAction::Status { roots } => {
                    let (manifests, warnings) = roots.service()?.deployed_with_warnings()?;
                    warn(&warnings);
                    print(serde_json::to_value(manifests)?)
                }
                DeploymentAction::Deploy {
                    roots,
                    component,
                    name,
                } => {
                    let service = roots.service()?;
                    let _lock = service.lock()?;
                    let (manifest, warnings) = service.deploy_report(&component, name)?;
                    warn(&warnings);
                    print(serde_json::to_value(manifest)?)
                }
                DeploymentAction::Uninstall { roots, component } => {
                    let service = roots.service()?;
                    let _lock = service.lock()?;
                    let report = service.uninstall_report(&component)?;
                    warn(&report.warnings);
                    print(serde_json::to_value(report)?)
                }
                DeploymentAction::Purge { roots } => {
                    let service = roots.service()?;
                    let _lock = service.lock()?;
                    warn(&service.deployed_with_warnings()?.1);
                    let reports = service.purge_report()?;
                    for report in &reports {
                        warn(&report.warnings);
                    }
                    print(serde_json::json!({"purged": reports.len(), "components": reports}))
                }
                DeploymentAction::Snapshot { roots, out } => {
                    let index = roots.service()?.capture_index()?;
                    recovery::atomic_write(&out, &serde_json::to_vec_pretty(&index)?)?;
                    print(serde_json::json!({"files": index.len(), "out": out}))
                }
                DeploymentAction::Record {
                    roots,
                    component,
                    name,
                    before,
                    backup_source,
                } => {
                    let before: modsync::deployment::HashIndex =
                        serde_json::from_slice(&std::fs::read(&before)?)?;
                    let service = roots.service()?;
                    let _lock = service.lock()?;
                    let (manifest, report) =
                        service.record_live(&component, name, &before, backup_source.as_deref())?;
                    warn(&report.warnings);
                    print(serde_json::json!({"provenance": report, "manifest": manifest}))
                }
                DeploymentAction::Switch {
                    game_dir,
                    settings_dir,
                    to,
                    purge_current,
                } => {
                    let settings = modsync::profiles::settings_directory(settings_dir.as_deref())?;
                    let store = modsync::profiles::Store::new(&settings)?;
                    let target_profile = store.require(&to)?;

                    // Preflight and lock the target first: a known-bad target must
                    // refuse before anything reads the active profile or touches the
                    // current deployment, and `active_name` is read only once the
                    // lock is held so a concurrent switch cannot interleave with it.
                    let target = modsync::deployment::Service::for_profile(
                        &settings,
                        &target_profile.name,
                        &game_dir,
                    )?;
                    let _lock = target.lock()?;

                    let from = store.active_name()?;
                    let switching = from
                        .as_deref()
                        .is_some_and(|current| !current.eq_ignore_ascii_case(to.trim()));

                    // Same per-game lock file as `target`'s; never lock it twice.
                    let source = switching
                        .then(|| {
                            modsync::deployment::Service::for_profile(
                                &settings,
                                from.as_deref().unwrap(),
                                &game_dir,
                            )
                        })
                        .transpose()?;

                    // Read-only target report prerequisites are checked before the
                    // purge below, so a known-bad target refuses without purging the
                    // current profile's deployments needlessly, and before persisting
                    // activation, so a corrupt target manifest cannot leave
                    // activeProfileName pointing at a profile we then fail to report.
                    let deployed = target.deployed()?.len();
                    let staged_not_deployed = target.staged_not_deployed()?;

                    let mut purged = Vec::new();
                    if let Some(service) = &source {
                        let current = from.as_deref().unwrap();

                        // Preflight every live manifest's reinstall blockers before any
                        // purge mutation: a hash mismatch, non-regular path, or unsafe
                        // path refuses cleanly, leaving game/manifests/backups/active
                        // profile untouched, instead of soft-skipping mid-purge and
                        // activating over a modified file left behind.
                        let (manifests, _) = service.deployed_with_warnings()?;
                        let mut blockers = Vec::new();
                        for manifest in &manifests {
                            blockers.extend(service.reinstall_blockers(&manifest.component_guid)?);
                        }
                        ensure!(
                            blockers.is_empty(),
                            "Cannot switch from profile '{current}': {}. Resolve these files, then retry; nothing was changed.",
                            blockers.join("; ")
                        );

                        if purge_current {
                            purged = service.purge_report()?;
                            for report in &purged {
                                warn(&report.warnings);
                            }
                            // A blocker can still appear between the preflight above and
                            // this purge (e.g. a hostile external writer); this is not
                            // promised atomic, but a skip or retained manifest here must
                            // still refuse activation rather than claim the purge finished.
                            let skipped = purged.iter().any(|report| !report.skipped.is_empty());
                            let live = service.deployed()?.len();
                            ensure!(
                                live == 0 && !skipped,
                                "Purge for profile '{current}' left {live} deployment(s) or skipped file(s) still live in this game directory; refusing to activate '{to}' while '{current}' remains live."
                            );
                        } else {
                            let live = service.deployed()?.len();
                            ensure!(
                                live == 0,
                                "Profile '{current}' still has {live} deployment(s) in this game directory. Two live profiles cannot share one game tree; pass --purge-current to remove them first."
                            );
                        }
                    }

                    let profile = store.activate(&to)?;
                    print(serde_json::json!({
                        "from": from,
                        "to": profile.name,
                        "purged": purged,
                        "deployed": deployed,
                        "stagedNotDeployed": staged_not_deployed,
                    }))
                }
            }
        }
        Some(Action::FomodInspect {
            archive,
            choices,
            extracted_directory,
            save_choices,
            interactive,
            scratch_dir,
            max_expanded_bytes,
        }) => {
            let mut module = modsync::fomod::inspect_archive(
                &archive,
                &modsync::archive::Options {
                    scratch_dir,
                    max_expanded_bytes,
                },
                &AtomicBool::new(false),
            )?;
            if let Some(path) = choices {
                let choices = modsync::fomod::ChoicesFile::parse(&std::fs::read_to_string(path)?)?;
                let filename = archive
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("Archive filename is not valid UTF-8")?;
                let selections = choices.archive(filename).with_context(|| {
                    format!("Choices file has no entry for archive '{filename}'")
                })?;
                selections.apply_to(
                    &mut module
                        .as_mut()
                        .context("Archive has no FOMOD installer to configure")?
                        .config
                        .steps,
                )?;
            }
            if interactive {
                modsync::fomod::prompt_choices(
                    &mut module
                        .as_mut()
                        .context("Archive has no FOMOD installer to configure")?
                        .config
                        .steps,
                    &mut std::io::stdin().lock(),
                    &mut std::io::stderr().lock(),
                )?;
            }
            let instructions = extracted_directory
                .as_deref()
                .map(|root| {
                    module
                        .as_ref()
                        .context("Archive has no FOMOD installer to map")?
                        .copy_instructions(root)
                })
                .transpose()?;
            if let Some(path) = save_choices {
                ensure!(
                    match path.canonicalize() {
                        Ok(output) => output != archive.canonicalize()?,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
                        Err(error) => return Err(error.into()),
                    },
                    "Choices output must not overwrite the archive"
                );
                let module = module
                    .as_ref()
                    .context("Archive has no FOMOD installer to save")?;
                let filename = archive
                    .file_name()
                    .and_then(|n| n.to_str())
                    .context("Archive filename is not valid UTF-8")?;
                let choices = modsync::fomod::ChoicesFile::capture(filename, &module.config.steps)?;
                recovery::atomic_write(
                    &std::path::absolute(path)?,
                    &serde_json::to_vec_pretty(&choices)?,
                )?;
            }
            let report = module.map(|m| {
                let visible = modsync::fomod::visible_steps(&m.config.steps);
                let steps: Vec<_> = m.config.steps.iter().enumerate().map(|(index, step)| {
                    let groups: Vec<_> = step.groups.iter().map(|group| {
                        let plugins: Vec<_> = group.plugins().iter().map(|plugin| serde_json::json!({
                            "name": plugin.name, "description": plugin.description,
                            "imagePath": plugin.image_path, "required": plugin.required,
                            "pluginType": plugin.plugin_type,
                            "selected": plugin.selected,
                        })).collect();
                        serde_json::json!({"name": group.name, "type": group.kind, "plugins": plugins})
                    }).collect();
                    serde_json::json!({"name": step.name, "visible": visible.contains(&index), "groups": groups})
                }).collect();
                serde_json::json!({
                    "modulePath": m.module_path, "contentRoot": m.content_root,
                    "name": m.config.name, "info": m.info, "steps": steps,
                    "instructions": instructions,
                })
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Some(Action::List { input }) => {
            println!("{}", serde_json::to_string_pretty(&Guide::load(&input)?)?);
            Ok(())
        }
        Some(Action::ProtocolParse { url }) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&modsync::protocol::parse(&url)?)?
            );
            Ok(())
        }
        Some(Action::ProtocolFetch { url, output }) => {
            modsync::protocol::fetch_instruction(&url, &output)?;
            println!("{}", output.display());
            Ok(())
        }
        Some(Action::ProtocolOpen { url }) => {
            let (link, guide) = modsync::protocol::fetch_modsync_guide(&url)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({"Link": link, "Guide": guide}))?
            );
            Ok(())
        }
        Some(Action::ProtocolResolve { input, url }) => {
            let guide = Guide::load(&input)?;
            let link = modsync::protocol::parse(&url)?;
            let (status, matched) = modsync::protocol::resolve_nxm(&link, &guide.components);
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "Status": status,
                    "Match": matched,
                }))?
            );
            Ok(())
        }
        Some(Action::ProtocolRegisterLinux {
            executable,
            applications_dir,
        }) => {
            let directory = applications_dir.unwrap_or_else(|| {
                dirs::data_local_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("applications")
            });
            println!(
                "{}",
                modsync::protocol_registration::register_linux(&executable, &directory)?.display()
            );
            Ok(())
        }
        Some(Action::NxmDownload {
            url,
            output,
            input,
            record,
            nexus_mods_api_key,
            settings_dir,
            nexus_api_base,
        }) => {
            let link = modsync::protocol::parse(&url)?;
            anyhow::ensure!(
                matches!(link, modsync::protocol::Link::Nxm { .. }),
                "Expected an nxm:// URL"
            );
            let key = if nexus_api_base.is_some() {
                // Never send a stored or environment key to a caller-chosen host.
                Some(
                    nexus_mods_api_key
                        .filter(|key| !key.trim().is_empty())
                        .context("--nexus-api-base requires an explicit --nexus-mods-api-key")?,
                )
            } else {
                // An explicit flag wins without reading settings.
                match nexus_mods_api_key {
                    Some(key) => Some(key),
                    None => {
                        let directory =
                            modsync::profiles::settings_directory(settings_dir.as_deref())?;
                        modsync::profiles::Store::new(&directory)?.nexus_api_key()?
                    }
                }
                .or_else(|| std::env::var("KOTOR_MODSYNC_NEXUS_API_KEY").ok())
                .or_else(|| std::env::var("NEXUS_MODS_API_KEY").ok())
            };
            if let Some(input) = input {
                use modsync::download_orchestrator as orchestrator;
                anyhow::ensure!(
                    output.is_dir(),
                    "With --input, OUTPUT must be the existing mod workspace directory"
                );
                let mut guide = Guide::load(&input)?;
                let target = orchestrator::nxm_target(&guide, &link)?;
                let plan = orchestrator::AcquisitionPlan::single(target, &output)?;
                let settings = orchestrator::RunSettings {
                    workers: 1,
                    nexus_api_key: key,
                    nexus_api_base,
                    ..Default::default()
                };
                let report = orchestrator::run(&plan, &settings, &AtomicBool::new(false), |_| {})?;
                if record {
                    let summary = orchestrator::record(&mut guide, &report)?;
                    if !summary.recorded.is_empty() {
                        guide.save(&input)?;
                    }
                }
                println!("{}", serde_json::to_string_pretty(&report)?);
                anyhow::ensure!(
                    report
                        .outcomes
                        .iter()
                        .all(|o| o.status == orchestrator::TargetStatus::Completed),
                    "The nxm download did not complete"
                );
                return Ok(());
            }
            let mut client = match nexus_api_base {
                Some(base) => modsync::updates::NexusClient::with_base_url(key, &base)?,
                None => modsync::updates::NexusClient::new(key)?,
            };
            let download_link = client.nxm_download_link(&link)?;
            download::fetch(&download_link.uri, &output, None, &AtomicBool::new(false))?;
            println!("{}", serde_json::to_string_pretty(&download_link)?);
            Ok(())
        }
        Some(Action::UpdateApply {
            input,
            versions,
            output,
            summary,
        }) => {
            let mut guide = Guide::load(&input)?;
            let report =
                modsync::updates::apply(&mut guide, &modsync::updates::load_versions(&versions)?);
            guide.save(&output)?;
            if let Some(path) = summary {
                recovery::atomic_write(
                    &std::path::absolute(path)?,
                    &serde_json::to_vec_pretty(&report)?,
                )?;
            }
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Some(Action::UpdateCheck {
            input,
            output,
            nexus_mods_api_key,
            settings_dir,
        }) => {
            let directory = modsync::profiles::settings_directory(settings_dir.as_deref())?;
            // An explicit flag wins without reading settings.
            let key = match nexus_mods_api_key {
                Some(key) => Some(key),
                None => modsync::profiles::Store::new(&directory)?.nexus_api_key()?,
            }
                .or_else(|| std::env::var("KOTOR_MODSYNC_NEXUS_API_KEY").ok())
                .or_else(|| std::env::var("NEXUS_MODS_API_KEY").ok())
                .filter(|key| !key.trim().is_empty())
                .context("Set nexusModsApiKey in settings, an API-key flag, or a Nexus API-key environment variable")?;
            let mut guide = Guide::load(&input)?;
            let mut client = modsync::updates::NexusClient::new(Some(key))?;
            let report = modsync::updates::check(&mut guide, &mut client);
            guide.save(&output)?;
            std::fs::create_dir_all(&directory)?;
            modsync::updates::save_summary(&directory, &report)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Some(Action::Merge {
            existing,
            incoming,
            output,
            use_existing_order,
            prefer_existing_fields,
            prefer_existing_name,
            prefer_existing_author,
            prefer_existing_description,
            prefer_existing_directions,
            prefer_existing_category,
            prefer_existing_tier,
            prefer_existing_installation_method,
            prefer_existing_instructions,
            prefer_existing_options,
            prefer_existing_resource_registry,
            prefer_incoming_fields: _,
            exclude_existing_only,
            exclude_incoming_only,
        }) => {
            let existing = Guide::load(&existing)?;
            let mut incoming = Guide::load(&incoming)?;
            incoming.components = modsync::merge::components(
                &existing.components,
                &incoming.components,
                &modsync::merge::Options {
                    use_existing_order,
                    prefer_existing_fields,
                    prefer_existing_name,
                    prefer_existing_author,
                    prefer_existing_description,
                    prefer_existing_directions,
                    prefer_existing_category,
                    prefer_existing_tier,
                    prefer_existing_installation_method,
                    prefer_existing_instructions,
                    prefer_existing_options,
                    prefer_existing_resource_registry,
                    exclude_existing_only,
                    exclude_incoming_only,
                    ..Default::default()
                },
            );
            incoming.save(&output)
        }
        Some(Action::Convert {
            input,
            output,
            input_path,
            output_path,
            stdin,
            format,
            parse_directions,
            auto_generate_local,
            source_path,
            review_draft,
        }) => {
            let mut guide = if stdin {
                use std::io::Read;
                let mut text = String::new();
                std::io::stdin().read_to_string(&mut text)?;
                Guide::parse(&text, "auto")?
            } else {
                Guide::load(
                    &input_path
                        .or(input)
                        .context("Use --input <file> or --stdin")?,
                )?
            };
            if parse_directions {
                modsync::draft::generate(&mut guide);
            }
            if auto_generate_local {
                let reports = modsync::archive_generation::generate_local(
                    &mut guide,
                    source_path.as_deref().unwrap(),
                )?;
                eprintln!("{}", serde_json::to_string_pretty(&reports)?);
            }
            for id in review_draft {
                let component = guide
                    .components
                    .iter_mut()
                    .find(|c| c.guid.eq_ignore_ascii_case(&id))
                    .with_context(|| format!("Unknown component GUID: {id}"))?;
                modsync::draft::mark_reviewed(component)?;
            }
            let format = format.as_deref().unwrap_or_else(|| {
                output
                    .as_ref()
                    .and_then(|p| p.extension())
                    .and_then(|s| s.to_str())
                    .unwrap_or("toml")
            });
            let text = guide.render(format)?;
            if let Some(path) = output_path.as_ref().or(output.as_ref()) {
                recovery::atomic_write(&std::path::absolute(path)?, text.as_bytes())?;
            } else {
                println!("{text}");
            }
            Ok(())
        }
        Some(Action::Restore {
            game_dir,
            source_dir,
        }) => recovery::restore(&Roots::new(&source_dir, &game_dir)?),
        Some(Action::CompletionSummary {
            game_dir,
            source_dir,
            json,
        }) => {
            let summary = recovery::completion_summary(&Roots::new(&source_dir, &game_dir)?)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else if let Some(summary) = summary {
                println!("Components in receipt: {}", summary.components.len());
                for component in &summary.components {
                    println!("  {} ({})", component.name, component.guid);
                }
                println!("Game files in receipt: {}", summary.game_files);
                match summary.game_delta_since_run_start {
                    Some(delta) => println!(
                        "Game files changed since run start: {} added, {} changed, {} removed",
                        delta.added, delta.changed, delta.removed
                    ),
                    None => println!("Game files changed since run start: N/A"),
                }
            } else {
                println!("No completion receipt.");
            }
            Ok(())
        }
        Some(Action::Profile(args)) => args.run(),
        Some(Action::Settings(args)) => args.run(),
        Some(Action::DownloadPlan {
            input,
            component,
            url,
            filenames,
            source_dir,
        }) => {
            let guide = Guide::load(&input)?;
            let c = guide
                .components
                .iter()
                .find(|c| c.guid.eq_ignore_ascii_case(&component))
                .context("Unknown component GUID")?;
            let selected = modsync::download_plan::select_filenames(
                c,
                &url,
                &filenames,
                source_dir.as_deref(),
            )?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"UrlEnabled": modsync::download_plan::url_enabled(c, &url), "Filenames": selected})
                )?
            );
            Ok(())
        }
        Some(Action::DownloadProvider { url }) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&modsync::download_hosts::provider(&url)?)?
            );
            Ok(())
        }
        Some(Action::FetchDownloads {
            input,
            source_dir,
            game_dir,
            use_file_selection,
            select,
            option,
            selection_flags,
            workers,
            retries,
            nexus_mods_api_key,
            settings_dir,
            nexus_api_base,
            extra_ca_file,
            peer_mode,
            scratch_dir,
            max_expanded_bytes,
            dry_run,
            progress_json,
            record,
        }) => {
            use modsync::{
                download_orchestrator as orchestrator, p2p, peer_acquire::PeerAcquire, selection,
            };
            // C1/C4: the loaded guide stays pristine for `record`/`save`; selection runs on
            // a separate, transient clone that plan/run/the Peer adapter/`fomod_followups`
            // see instead. No selection change from this run is ever written back.
            let mut guide = Guide::load_preserving_selection(&input)?;
            let mut execution = guide.clone();
            let has_selector = !select.is_empty()
                || !option.is_empty()
                || !selection_flags.select_through_tier.is_empty()
                || !selection_flags.add_category.is_empty()
                || !selection_flags.select_visible_search.is_empty();
            if !use_file_selection {
                for c in &mut execution.components {
                    c.is_selected = !has_selector;
                }
            }
            let detected = game_dir
                .as_deref()
                .map(|dir| modsync::game::detect(&modsync::game::resolve_install_root(dir)));
            apply_selection(
                &mut execution.components,
                detected,
                &select,
                &option,
                &selection_flags,
                true,
            )?;
            let candidates =
                selection::download_candidates(&execution.components, selection::Scope::All);

            // C2: the effective peer settings/mode, resolved before the plan. An explicit
            // --peer-mode overrides the stored mode; peerSharing=false always wins. An
            // explicit `--peer-mode off` needs no settings at all, so it bypasses even
            // unreadable (malformed) settings.json, matching the explicit-key/API-base
            // credential bypass in C3 below.
            let (effective_mode, peer_settings) = match peer_mode {
                Some(PeerModeArg::Off) => (p2p::PeerMode::Off, None),
                _ => {
                    let settings_value =
                        modsync::profiles::SettingsFile::resolve(settings_dir.as_deref())?
                            .load()?;
                    let mut peer_settings = p2p::PeerSettings::from_settings(&settings_value);
                    if let Some(mode) = peer_mode {
                        peer_settings.mode = mode.into();
                    }
                    let effective_mode = peer_settings.effective_mode();
                    (effective_mode, Some(peer_settings))
                }
            };
            let orchestrator_peer_mode = match effective_mode {
                p2p::PeerMode::Off => orchestrator::PeerMode::Off,
                p2p::PeerMode::Race => orchestrator::PeerMode::Race,
                p2p::PeerMode::Fallback => orchestrator::PeerMode::Fallback,
            };

            // C3: a stored/legacy key is looked up only when a selected candidate's
            // resource actually needs Nexus and no explicit key was given. An overridden
            // API base always requires an explicit, nonblank key.
            let needs_nexus = candidates.iter().any(|candidate| {
                execution.components[candidate.index]
                    .resource_registry
                    .keys()
                    .any(|url| {
                        matches!(
                            modsync::download_hosts::provider(url),
                            Ok(modsync::download_hosts::Provider::Nexus)
                        )
                    })
            });
            let key = if nexus_api_base.is_some() {
                Some(
                    nexus_mods_api_key
                        .filter(|key| !key.trim().is_empty())
                        .context("--nexus-api-base requires an explicit --nexus-mods-api-key")?,
                )
            } else {
                match nexus_mods_api_key {
                    Some(key) => Some(key),
                    None if needs_nexus => {
                        let directory =
                            modsync::profiles::settings_directory(settings_dir.as_deref())?;
                        modsync::profiles::Store::new(&directory)?.nexus_api_key()?
                    }
                    None => None,
                }
            };

            let archive_options = modsync::archive::Options {
                scratch_dir: scratch_dir.clone(),
                max_expanded_bytes,
            };
            let plan_options = orchestrator::PlanOptions {
                source_dir: source_dir.clone(),
                resolution: None,
                peer_mode: orchestrator_peer_mode,
                user_peers: Vec::new(),
                nexus_key_present: key.is_some(),
                archive: archive_options.clone(),
            };
            let plan = orchestrator::plan(&execution, &candidates, &plan_options)?;

            let loopback_ca = extra_ca_file
                .as_deref()
                .map(modsync::download::LoopbackCa::from_pem_file)
                .transpose()?;

            // C4: dry-run prints the effective plan and returns before any policy
            // creation, signal handling, save or FOMOD extraction.
            if dry_run {
                if progress_json {
                    let envelope = serde_json::json!({"type": "plan", "plan": plan});
                    println!("{}", serde_json::to_string(&envelope)?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                }
                return Ok(());
            }

            let peer: Option<Arc<dyn modsync::acquisition::Acquire>> =
                match (effective_mode, peer_settings) {
                    (p2p::PeerMode::Off, _) => None,
                    (_, Some(peer_settings)) => Some(Arc::new(PeerAcquire::new(
                        &execution,
                        peer_settings,
                        archive_options.clone(),
                        settings_dir.clone(),
                    ))),
                    // Unreachable: effective_mode is only non-Off when peer_settings was
                    // actually read (see above).
                    (_, None) => None,
                };
            let run_settings = orchestrator::RunSettings {
                workers,
                retries,
                archive: archive_options.clone(),
                nexus_api_key: key,
                nexus_api_base,
                peer,
            };
            let cancelled = p2p::stop_signal_flag()?;
            let stdout = std::sync::Mutex::new(std::io::stdout());
            let report = orchestrator::run_with_loopback_ca(
                &plan,
                &run_settings,
                &cancelled,
                |event| {
                    if progress_json {
                        let envelope = serde_json::json!({"type": "acquisition", "event": event});
                        if let Ok(line) = serde_json::to_string(&envelope) {
                            let mut out = stdout.lock().unwrap();
                            let _ = writeln!(out, "{line}");
                            let _ = out.flush();
                        }
                    }
                },
                loopback_ca.as_ref(),
            )?;

            let record_summary = if record {
                let summary = orchestrator::record(&mut guide, &report)?;
                if !summary.recorded.is_empty() {
                    guide.save(&input)?;
                }
                Some(summary)
            } else {
                None
            };
            let fomod =
                orchestrator::fomod_followups(&execution, &report, &archive_options, &cancelled);

            let summary_value = serde_json::json!({
                "report": report,
                "recordSummary": record_summary,
                "fomodFollowups": fomod,
            });
            if progress_json {
                let envelope = serde_json::json!({"type": "summary", "summary": summary_value});
                println!("{}", serde_json::to_string(&envelope)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&summary_value)?);
            }

            if report.cancelled || cancelled.load(Ordering::SeqCst) {
                std::process::exit(130);
            }
            let failed = report
                .outcomes
                .iter()
                .any(|o| o.status == orchestrator::TargetStatus::Failed);
            if failed || !fomod.inspection_errors.is_empty() {
                std::process::exit(1);
            }
            let manual = report
                .outcomes
                .iter()
                .any(|o| o.status == orchestrator::TargetStatus::ManualRequired);
            if manual || !fomod.followups.is_empty() {
                std::process::exit(3);
            }
            Ok(())
        }
        Some(Action::Download {
            url,
            output,
            sha256,
            max_expanded_bytes,
        }) => download::fetch_with_options(
            &url,
            &output,
            sha256.as_deref(),
            &AtomicBool::new(false),
            &modsync::archive::Options {
                max_expanded_bytes,
                ..Default::default()
            },
        ),
        Some(Action::TorrentDownload {
            magnet,
            output,
            sha256,
            peer,
            max_expanded_bytes,
            torrent_info,
            proxy,
        }) => modsync::p2p::cli_download(
            &magnet,
            torrent_info.as_deref(),
            &output,
            &sha256,
            peer,
            &proxy.into(),
            &modsync::archive::Options {
                max_expanded_bytes,
                ..Default::default()
            },
        ),
        Some(Action::TorrentSeed {
            archive,
            tracker,
            authored_magnet,
            torrent_info,
            proxy,
        }) => modsync::p2p::cli_seed(
            &archive,
            tracker,
            authored_magnet.as_deref(),
            torrent_info.as_deref(),
            &proxy.into(),
        ),
        Some(Action::DownloadBatch {
            input,
            workers,
            retries,
            scratch_dir,
            max_expanded_bytes,
        }) => {
            let jobs = serde_json::from_slice(&std::fs::read(input)?)?;
            let results = modsync::download_queue::fetch_all(
                jobs,
                workers,
                retries,
                &AtomicBool::new(false),
                modsync::archive::Options {
                    scratch_dir,
                    max_expanded_bytes,
                },
            )?;
            println!("{}", serde_json::to_string_pretty(&results)?);
            ensure!(
                results
                    .iter()
                    .all(|item| item.status == modsync::download_queue::Status::Completed),
                "One or more downloads failed"
            );
            Ok(())
        }
        Some(Action::Selection(args)) => {
            use modsync::selection;
            let mut guide = Guide::load_for_install(&args.input)?;
            if args.select.is_empty() && !args.use_file_selection {
                for c in &mut guide.components {
                    c.is_selected = true;
                }
            }
            let detected = args
                .game_dir
                .as_deref()
                .map(|dir| modsync::game::detect(&modsync::game::resolve_install_root(dir)));
            let outcomes = apply_selection(
                &mut guide.components,
                detected,
                &args.select,
                &args.option,
                &args.selection_flags,
                true,
            )?;
            let components = &guide.components;
            let counts = selection::counts(components);
            let search = args
                .selection_flags
                .select_visible_search
                .last()
                .map_or("", String::as_str);
            let summary = selection::filter_summary(
                components,
                search,
                "",
                "",
                args.selection_flags.spoiler_free,
            );
            let selected = selection::selected_guids(components);
            if args.json {
                let candidates = |scope| selection::download_candidates(components, scope);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "game": detected.map(|game| format!("{game:?}")),
                        "counts": counts,
                        "filterSummary": summary,
                        "selected": selected,
                        "outcomes": outcomes,
                        "downloadCandidates": {
                            "base": candidates(selection::Scope::Base),
                            "widescreen": candidates(selection::Scope::Widescreen),
                            "all": candidates(selection::Scope::All),
                        },
                    }))?
                );
            } else {
                println!(
                    "{} — {}",
                    selection::summary_count(&counts),
                    selection::summary_detail(&counts)
                );
                println!(
                    "SELECTED {}  REQUIRED {}  OPTIONAL {}",
                    counts.selected, counts.required, counts.optional
                );
                println!("{summary}");
                for guid in &selected {
                    println!("{}", selection::describe(components, guid));
                }
            }
            Ok(())
        }
        Some(action @ (Action::Install(_) | Action::Validate(_) | Action::Conflicts(_))) => {
            let install = matches!(action, Action::Install(_));
            let conflicts = matches!(action, Action::Conflicts(_));
            let (Action::Install(args) | Action::Validate(args) | Action::Conflicts(args)) = action
            else {
                unreachable!()
            };
            ensure!(
                !args.install_progress_json || install,
                "--install-progress-json is only supported by install"
            );
            let report_mode = args.validation_report && !install && !conflicts;
            let dispatch = (|| -> Result<()> {
                ensure!(
                    args.phase == PhaseArg::All || !args.direct_markdown,
                    "Phased Direct Markdown is unsupported: separate-invocation definitions are not stable"
                );
                ensure!(
                    args.phase == PhaseArg::All
                        || !(install || conflicts)
                        || !args.validation_report,
                    "Validation-report flags are only supported by phased validate"
                );
                ensure!(
                    args.phase != PhaseArg::Remaining || !args.dry_run_only,
                    "Remaining reports always run full validation and dry run; --dry-run-only is incompatible"
                );
                let settings_directory =
                    if args.settings_dir.is_some() || args.profile.is_some() || args.active_profile
                    {
                        Some(modsync::profiles::settings_directory(
                            args.settings_dir.as_deref(),
                        )?)
                    } else {
                        None
                    };
                let patcher_preferences = settings_directory
                    .as_deref()
                    .map(modsync::profiles::Store::new)
                    .transpose()?
                    .map(|store| store.patcher_preferences())
                    .transpose()?;
                let patcher_engine = args
                    .patcher_engine
                    .map(Into::into)
                    .or_else(|| {
                        patcher_preferences
                            .as_ref()
                            .map(|preferences| preferences.engine)
                    })
                    .unwrap_or_default();
                let profile = if args.profile.is_some() || args.active_profile {
                    let directory = settings_directory.as_ref().unwrap();
                    let store = modsync::profiles::Store::new(directory)?;
                    let name = if args.active_profile {
                        store
                            .active_name()?
                            .context("No active profile is configured")?
                    } else {
                        args.profile.clone().unwrap()
                    };
                    Some(store.require(&name)?)
                } else {
                    None
                };
                let paths = if let Some(profile) = &profile {
                    profile.install_paths(
                        args.input.as_deref(),
                        args.game_dir.as_deref(),
                        args.source_dir.as_deref(),
                    )?
                } else {
                    modsync::profiles::InstallPaths {
                        input: args.input.clone().context("Supply --input")?,
                        game: args.game_dir.clone().context("Supply --game-dir")?,
                        source: args.source_dir.clone().context("Supply --source-dir")?,
                    }
                };
                if args.direct_markdown {
                    ensure!(
                        paths
                            .input
                            .extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("md")),
                        "Direct Markdown mode requires a .md input file"
                    );
                }
                let mut guide = Guide::load_for_install(&paths.input)?;
                if let Some(profile) = &profile {
                    profile.apply(&mut guide);
                }
                if args.select.is_empty() {
                    for c in &mut guide.components {
                        if !args.use_file_selection && profile.is_none() {
                            c.is_selected = true;
                        }
                    }
                }
                let detected =
                    modsync::game::detect(&modsync::game::resolve_install_root(&paths.game));
                apply_selection(
                    &mut guide.components,
                    Some(detected),
                    &args.select,
                    &args.option,
                    &args.selection_flags,
                    !report_mode || args.phase == PhaseArg::All,
                )?;
                if args.direct_markdown {
                    ensure!(
                        patcher_engine == PatcherEngine::OdyPatcher,
                        "Direct Markdown mode requires --patcher-engine ody-patcher"
                    );
                    let result = modsync::direct_markdown::apply(&mut guide.components);
                    eprintln!(
                        "Direct Markdown NLP: draftedFromProse={}, 4GB Patcher skipped={}, widescreen skipped={}",
                        result.drafted_from_prose,
                        result.skipped_four_gb,
                        result.skipped_widescreen
                    );
                    ensure!(
                        result.ready(),
                        "Direct Markdown preflight stopped before game writes: {} selected component(s) have no reviewed executable actions: {}",
                        result.unresolved_components.len(),
                        result
                            .unresolved_components
                            .iter()
                            .take(25)
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                let choices = args
                    .fomod_choices
                    .as_ref()
                    .map(|path| modsync::fomod::ChoicesFile::parse(&std::fs::read_to_string(path)?))
                    .transpose()?;
                let context = if args.phase == PhaseArg::Remaining {
                    modsync::phase::CompletionContext::Reconstruct
                } else {
                    modsync::phase::CompletionContext::None
                };
                let prepared =
                    modsync::phase::prepare(&guide, args.phase.into(), context, choices.as_ref())?;
                let guide = prepared.guide;
                let mut settings = Settings::new(&paths.source, &paths.game)?;
                settings.allow_processes = args.allow_processes;
                settings.patcher = args.patcher.or_else(|| {
                    patcher_preferences
                        .filter(|preferences| preferences.engine == patcher_engine)
                        .and_then(|preferences| preferences.executable.map(PathBuf::from))
                });
                settings.patcher_engine = patcher_engine;
                settings.archive.scratch_dir = args.scratch_dir;
                settings.archive.max_expanded_bytes = args.max_expanded_bytes;
                let engine = Engine::new(settings)?;
                let emit = |event: modsync::Event| {
                    if args.validation_report {
                        return;
                    }
                    if args.install_progress_json {
                        // Keep stdout typed; external program output stays readable.
                        eprintln!("{} / {}: {}", event.component, event.step, event.message);
                        return;
                    }
                    if args.json {
                        println!("{}", serde_json::to_string(&event).unwrap());
                    } else {
                        println!("{} / {}: {}", event.component, event.step, event.message);
                    }
                };
                if conflicts {
                    let report = if args.phase == PhaseArg::Remaining {
                        engine.analyze_conflicts_remaining(&guide, &prepared.completed)?
                    } else {
                        engine.analyze_conflicts(&guide)?
                    };
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    Ok(())
                } else if install {
                    if args.install_progress_json {
                        let progress = |progress: modsync::install_progress::Progress| {
                            let envelope = serde_json::json!({
                                "type": "installProgress",
                                "progress": progress,
                            });
                            if let Ok(mut line) = serde_json::to_vec(&envelope) {
                                line.push(b'\n');
                                let _ = std::io::stdout().lock().write_all(&line);
                            }
                        };
                        if args.phase == PhaseArg::Remaining {
                            engine.install_remaining_observed(
                                &guide,
                                &prepared.completed,
                                emit,
                                progress,
                            )
                        } else {
                            engine.install_observed(&guide, emit, progress)
                        }
                    } else if args.phase == PhaseArg::Remaining {
                        engine.install_remaining(&guide, &prepared.completed, emit)
                    } else {
                        engine.install(&guide, emit)
                    }
                } else if args.validation_report {
                    let mut report = if args.phase == PhaseArg::Remaining {
                        engine.validate_report_remaining(&guide, &prepared.completed, emit)?
                    } else {
                        engine.validate_report_with_options(
                            &guide,
                            emit,
                            args.full,
                            args.dry_run || args.dry_run_only,
                            args.dry_run_only,
                        )
                    };
                    report.input_path = Some(paths.input.to_string_lossy().into_owned());
                    println!("{}", serde_json::to_string_pretty(&report)?);
                    if !report.success {
                        // The pipeline report is the machine output; a failing report
                        // exits nonzero without an extra stderr line, matching C#.
                        std::process::exit(1);
                    }
                    Ok(())
                } else if args.phase == PhaseArg::Remaining {
                    engine.validate_remaining(&guide, &prepared.completed, emit)
                } else {
                    engine.validate(&guide, emit)
                }
            })();
            if report_mode {
                if let Err(error) = dispatch {
                    // Setup failed before any pipeline stage ran; emit the C#
                    // SerializeError shape as the sole machine output.
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&modsync::validation::ErrorReport::new(
                            format!("{error:#}")
                        ))?
                    );
                    std::process::exit(1);
                }
                Ok(())
            } else {
                dispatch
            }
        }
    }
}

fn gui(
    input: Option<PathBuf>,
    game: Option<PathBuf>,
    mods: Option<PathBuf>,
    settings_dir: Option<PathBuf>,
    protocol: Option<String>,
    allow_multiple_instances: bool,
) -> Result<()> {
    #[cfg(feature = "desktop")]
    {
        ui::launch(
            input,
            game,
            mods,
            settings_dir,
            protocol,
            allow_multiple_instances,
        )
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (
            input,
            game,
            mods,
            settings_dir,
            protocol,
            allow_multiple_instances,
        );
        anyhow::bail!(
            "Desktop support is disabled. Build with the default features, or use --help for CLI commands."
        )
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
