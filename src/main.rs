mod checkpoint_cli;
mod cli_acquisition;
mod cli_compat;
mod cli_session;
mod maintenance_cli;
mod profile_cli;
mod protocol_cli;
use anyhow::{Context, Result, ensure};
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
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
    /// Include diagnostic log output on stderr without changing structured stdout.
    #[arg(short = 'v', long, global = true)]
    verbose: bool,
    /// Use plain text progress output (the CLI default).
    #[arg(long, global = true)]
    plaintext: bool,
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    /// Manage non-destructive staged deployments and their manifests.
    Deployment(DeploymentArgs),
    /// Manage stored install profiles.
    Profile(profile_cli::ProfileArgs),
    /// Get, set or list settings.json values.
    Settings(profile_cli::SettingsArgs),
    /// Inspect or manage per-user protocol associations owned by this package.
    Protocol(protocol_cli::ProtocolArgs),
    /// Validate and store a Nexus Mods API key.
    SetNexusApiKey(profile_cli::SetNexusApiKeyArgs),
    /// Capture, inspect and verify checkpoint history for explicit roots.
    Checkpoint(checkpoint_cli::CheckpointArgs),
    /// Preview, apply, inspect or recover explicit filesystem maintenance.
    Maintenance(maintenance_cli::MaintenanceArgs),
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
        /// Probe-only explicit Nexus key; never read from or written to settings.
        #[arg(long, hide = true)]
        nexus_mods_api_key: Option<String>,
        /// Probe-only API base override (HTTPS, or loopback HTTP); requires an explicit key.
        #[arg(long, hide = true)]
        nexus_api_base: Option<String>,
        /// Per-invocation CA for isolated loopback HTTPS archive probes.
        #[arg(long, hide = true)]
        extra_ca_file: Option<PathBuf>,
    },
    /// Run an explicitly trusted HoloPatcher backend outside a guide install.
    #[command(name = "holopatcher")]
    HoloPatcher {
        #[arg(long)]
        patcher: Option<PathBuf>,
        /// Explicitly authorize this backend to execute with your user permissions.
        #[arg(long)]
        allow_processes: bool,
        /// Legacy argument string; parsed without a shell.
        #[arg(
            short = 'a',
            long = "args",
            conflicts_with = "arguments",
            allow_hyphen_values = true
        )]
        legacy_arguments: Option<String>,
        #[arg(long)]
        working_directory: Option<PathBuf>,
        #[arg(long, default_value_t = 3600, value_parser = clap::value_parser!(u64).range(1..=86400))]
        timeout_seconds: u64,
        /// Backend argv following --, preserved as native OS strings.
        #[arg(last = true, allow_hyphen_values = true)]
        arguments: Vec<OsString>,
    },
    /// Explain the unsupported reference Python.Included bootstrap operation.
    InstallPythonDeps {
        #[arg(long)]
        force: bool,
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
    /// Preview a merge with field, resource, option and heuristic decisions.
    MergePreview {
        existing: PathBuf,
        incoming: PathBuf,
        /// JSON merge Options; omitted fields use production defaults.
        #[arg(long)]
        options: Option<PathBuf>,
    },
    /// Revalidate both guide identities and save an approved merge preview.
    MergeApply {
        existing: PathBuf,
        incoming: PathBuf,
        preview: PathBuf,
        output: PathBuf,
    },
    /// Prepare a non-approved resolution template for one generated component or option.
    GuideDraftReview { input: PathBuf, component: String },
    /// Apply an ordered JSON edit transaction and atomically save a distinct output.
    GuideEdit {
        input: PathBuf,
        operations: PathBuf,
        output: PathBuf,
    },
    /// Preview a local or previously fetched component document without editing.
    GuideImportPreview {
        input: PathBuf,
        document: PathBuf,
        #[arg(long, default_value = "auto")]
        format: String,
        /// A bounded custom regex profile; output includes match highlights.
        #[arg(long, conflicts_with = "format")]
        regex_profile: Option<PathBuf>,
        #[arg(long, conflicts_with_all = ["parent", "index"])]
        replace: Option<String>,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long)]
        index: Option<usize>,
    },
    /// Apply a saved import preview only if the original guide has not changed.
    GuideImportApply {
        input: PathBuf,
        preview: PathBuf,
        output: PathBuf,
    },
    /// Browse exact registered resources and archive inventories without extracting.
    GuideResources {
        input: PathBuf,
        component: String,
        #[arg(long)]
        source_dir: PathBuf,
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long, default_value_t = 268_435_456)]
        max_expanded_bytes: u64,
    },
    /// Read the timestamped update snapshot without changing guide metadata.
    UpdateSummary {
        input: PathBuf,
        #[arg(long)]
        settings_dir: Option<PathBuf>,
    },
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
        /// Declared mod root for a nested archive output without --input.
        #[arg(long, conflicts_with = "input")]
        source_dir: Option<PathBuf>,
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
        #[arg(required_unless_present = "existing_path")]
        existing: Option<PathBuf>,
        #[arg(required_unless_present = "incoming_path")]
        incoming: Option<PathBuf>,
        output: Option<PathBuf>,
        #[arg(short = 'e', long = "existing", conflicts_with = "existing")]
        existing_path: Option<PathBuf>,
        #[arg(short = 'n', long = "incoming", conflicts_with = "incoming")]
        incoming_path: Option<PathBuf>,
        #[arg(short = 'o', long = "output", conflicts_with = "output")]
        output_path: Option<PathBuf>,
        /// Output format; defaults to TOML.
        #[arg(short, long)]
        format: Option<String>,
        #[command(flatten)]
        processing: ConversionProcessing,
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
    /// Use convert --merge --existing <guide> --incoming <guide> for legacy merge mode.
    Convert {
        input: Option<PathBuf>,
        output: Option<PathBuf>,
        #[arg(short = 'i', long = "input", conflicts_with_all = ["input", "stdin"])]
        input_path: Option<PathBuf>,
        #[arg(short = 'o', long = "output", conflicts_with = "output")]
        output_path: Option<PathBuf>,
        #[arg(long, conflicts_with = "input")]
        stdin: bool,
        /// Output format; defaults to TOML.
        #[arg(short, long)]
        format: Option<String>,
        /// Draft instructions from prose; generated steps require review before installation.
        #[arg(long)]
        parse_directions: bool,
        /// Mark a component's displayed drafts reviewed (GUID; repeat for multiple components).
        #[arg(long)]
        review_draft: Vec<String>,
        /// Order around missing InstallBefore/InstallAfter references instead of failing.
        #[arg(long)]
        ignore_errors: bool,
        #[command(flatten)]
        processing: ConversionProcessing,
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
        /// Coordinate nested outputs with installation/maintenance of this root.
        #[arg(long)]
        source_dir: Option<PathBuf>,
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
        /// Coordinate nested outputs with installation/maintenance of this root.
        #[arg(long)]
        source_dir: Option<PathBuf>,
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
        /// Every output must be beneath this existing mod root.
        #[arg(long)]
        source_dir: Option<PathBuf>,
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
        /// Probe-only DeadlyStream base (HTTPS loopback only): every DeadlyStream page,
        /// selection and archive request goes there. Requires --extra-ca-file.
        #[arg(long, hide = true)]
        deadlystream_base: Option<String>,
        /// Probe-only MEGA API base (HTTPS loopback only): the API request and every
        /// download address it returns must be loopback HTTPS. Requires --extra-ca-file.
        #[arg(long, hide = true)]
        mega_api_base: Option<String>,
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
        /// Leave detected FOMOD installers unconfigured and mark them dismissed.
        #[arg(long)]
        fomod_skip: bool,
        /// Version-1 FOMOD choices JSON, overriding MODSYNC_FOMOD_CHOICES.
        #[arg(long)]
        fomod_choices: Option<PathBuf>,
        /// Ask to configure downloaded FOMOD installers, including with redirected input.
        #[arg(long, conflicts_with = "non_interactive")]
        interactive: bool,
        /// Disable automatic terminal FOMOD prompting.
        #[arg(long, conflicts_with = "interactive")]
        non_interactive: bool,
        /// Save acquisition facts and accepted FOMOD configuration/status edits.
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
struct ConversionProcessing {
    /// Version-1 FOMOD choices for the download prephase.
    #[arg(long)]
    fomod_choices: Option<PathBuf>,
    /// Suppress post-download configuration; unresolved installers still block processing.
    #[arg(long)]
    fomod_skip: bool,
    #[arg(long, conflicts_with = "non_interactive")]
    interactive: bool,
    #[arg(long, conflicts_with = "interactive")]
    non_interactive: bool,

    /// Reset output selection by name/GUID, category:Name, or tier:Name (repeatable).
    #[arg(short = 's', long)]
    select: Vec<String>,
    /// Infer missing instructions from local downloaded archives.
    #[arg(long, requires = "source_path")]
    auto_generate_local: bool,
    /// Infer review-required instructions from resolved resource filenames.
    #[arg(short = 'a', long = "auto")]
    auto_generate: bool,
    /// Download all guide resources before generating or rendering output.
    #[arg(short = 'd', long, requires = "source_path")]
    download: bool,
    #[arg(long)]
    source_path: Option<PathBuf>,
    /// Use four acquisition workers instead of one.
    #[arg(long, requires = "download")]
    concurrent: bool,
    #[arg(long)]
    nexus_mods_api_key: Option<String>,
    #[arg(long)]
    settings_dir: Option<PathBuf>,
    /// Overlay the five authored spoiler-free fields from a separate Markdown guide.
    #[arg(long)]
    spoiler_free: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ValidationOutput {
    Text,
    Json,
}

#[derive(Args)]
struct InstallArgs {
    /// Suppress post-download configuration; unresolved installers still block installation.
    #[arg(long)]
    fomod_skip: bool,
    #[arg(long, conflicts_with = "non_interactive")]
    interactive: bool,
    #[arg(long, conflicts_with = "interactive")]
    non_interactive: bool,

    /// Acquire selected resources before validation and the installation snapshot.
    #[arg(short = 'd', long)]
    download: bool,
    #[arg(long, visible_alias = "nexus-api-key")]
    nexus_mods_api_key: Option<String>,
    /// Use four download workers instead of one.
    #[arg(long, requires = "download")]
    concurrent: bool,
    /// Maximum hours for acquisition; zero uses the reference 48-hour default.
    #[arg(long, default_value_t = 48)]
    download_timeout_hours: u32,
    /// Retain a verified checkpoint after every successful top-level component.
    /// Failed operations require checkpoint recovery before retrying.
    #[arg(long)]
    component_checkpoints: bool,
    /// Keep whole-run recovery without retaining per-component history.
    #[arg(long, conflicts_with_all = ["component_checkpoints", "managed"])]
    no_checkpoint: bool,
    /// Compatibility flag; installs reject continuing after a failed step.
    #[arg(long, visible_alias = "continue-on-error")]
    best_effort: bool,
    // Reference install flags that skip validation, auto-confirm or continue
    // past a failure. Parsed only so they are refused by name.
    #[arg(long, hide = true)]
    skip_validation: bool,
    #[arg(long, hide = true)]
    continue_on_missing_sources: bool,
    #[arg(long, hide = true)]
    continue_on_mod_failure: bool,
    #[arg(long, hide = true)]
    ignore_errors: bool,
    #[arg(short = 'y', long, hide = true)]
    yes: bool,
    /// Stage and track managed deployment effects within component checkpoints.
    #[arg(long)]
    managed: bool,
    /// Managed staging root; supply together with --manifest-dir.
    #[arg(long, requires_all = ["managed", "manifest_dir"])]
    staging_dir: Option<PathBuf>,
    /// Managed manifest root; supply together with --staging-dir.
    #[arg(long, requires_all = ["managed", "staging_dir"])]
    manifest_dir: Option<PathBuf>,
    /// Run all selected components, the base, or the verified remaining suffix.
    #[arg(long, value_enum, default_value = "all")]
    phase: PhaseArg,
    /// Install a Markdown guide in source order after direct-Markdown preflight.
    #[arg(long, conflicts_with_all = ["fomod_choices", "profile", "active_profile", "use_file_selection", "select", "option", "deselect", "deselect_option", "select_through_tier", "add_category", "select_visible_search", "spoiler_free"])]
    direct_markdown: bool,
    /// Version-1 FOMOD choices JSON for this run, merged per archive: an entry replaces
    /// that archive's embedded choices and other archives keep theirs. Never saved.
    #[arg(long)]
    fomod_choices: Option<PathBuf>,
    #[arg(long, short, required_unless_present_any = ["profile", "active_profile"])]
    input: Option<PathBuf>,
    #[arg(long, short = 'g', required_unless_present_any = ["profile", "active_profile"])]
    game_dir: Option<PathBuf>,
    /// Mod files directory; install defaults to the input guide's parent directory.
    #[arg(long, short = 's')]
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
    #[arg(long, value_enum, ignore_case = true)]
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
    /// Validation report output, compatible with the reference text/json option.
    #[arg(long = "output", value_enum, ignore_case = true, conflicts_with_all = ["json", "validation_report", "install_progress_json"])]
    validation_output: Option<ValidationOutput>,
    /// Suppress successful stage and warning text in validation output.
    #[arg(long, conflicts_with = "json")]
    errors_only: bool,
    /// Include a non-mutating engine dry-run result in the validation report.
    #[arg(long, conflicts_with = "json")]
    dry_run: bool,
    /// Include environment, conflict, and install-order validation stages.
    #[arg(long, conflicts_with = "json")]
    full: bool,
    /// Run only FOMOD configuration and VFS dry-run validation.
    #[arg(long, conflicts_with = "json")]
    dry_run_only: bool,
}

/// Report selectors apply only to validate; installation event formats have
/// their own independent flags and must not be suppressed by report selectors.
fn validation_only_flags(args: &InstallArgs) -> bool {
    args.validation_report
        || args.validation_output.is_some()
        || args.errors_only
        || args.full
        || args.dry_run
        || args.dry_run_only
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
    let matches = command_definition()
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

/// Load the guide for validate, install and conflicts. Without a profile this is the
/// reference file loader, which selects every component when a non-Markdown file
/// selects none, so `--use-file-selection` then sees them all, as in C#. A profile
/// replaces the selection after loading, so its runs keep the authored flags.
fn load_validation_guide(input: &std::path::Path, profile: bool) -> Result<Guide> {
    if profile {
        Guide::load_for_install_preserving_selection(input)
    } else {
        Guide::load_like_file_loader(input)
    }
}

fn require_validation_selection(guide: &Guide, args: &InstallArgs) -> Result<()> {
    ensure!(
        guide
            .components
            .iter()
            .any(|component| component.is_selected),
        "{}",
        if !args.select.is_empty() {
            "No components match the selection criteria."
        } else if args.use_file_selection {
            "No components are marked IsSelected in the instruction file."
        } else {
            "No components are selected for validation."
        }
    );
    Ok(())
}

fn present_validation_report(
    report: &modsync::validation::Report,
    errors_only: bool,
    report_mode: bool,
) -> Result<()> {
    if report_mode {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        for stage in &report.stages {
            if !errors_only {
                println!("{:?}: {}", stage.stage, stage.summary);
            }
            for message in &stage.messages {
                if !errors_only
                    || (!message.starts_with("WARNING:")
                        && !message.starts_with("INFO:")
                        && (message.starts_with("ERROR:") || !stage.passed))
                {
                    println!("{message}");
                }
            }
        }
        if !errors_only {
            println!(
                "Validation {}: {} errors, {} warnings",
                if report.success { "passed" } else { "failed" },
                report.error_count,
                report.warning_count
            );
        }
    }
    if !report.success {
        // A failing report owns the diagnostics; Exit avoids duplicate stderr.
        return Err(cli_session::Exit(1).into());
    }
    Ok(())
}

fn validate_without_roots(
    args: &InstallArgs,
    profile: Option<&modsync::profiles::Profile>,
    game: Option<&std::path::Path>,
    report_mode: bool,
    cancelled: &AtomicBool,
    report_emitted: &mut bool,
) -> Result<()> {
    let unsupported = [
        (args.download, "--download"),
        (args.nexus_mods_api_key.is_some(), "--nexus-mods-api-key"),
        (args.concurrent, "--concurrent"),
        // Omitted and explicit default-equivalent 48 are the same inert value.
        (
            args.download_timeout_hours != 48,
            "--download-timeout-hours",
        ),
        (args.component_checkpoints, "--component-checkpoints"),
        (args.no_checkpoint, "--no-checkpoint"),
        (args.best_effort, "--best-effort"),
        (args.managed, "--managed"),
        (args.staging_dir.is_some(), "--staging-dir"),
        (args.manifest_dir.is_some(), "--manifest-dir"),
        (args.fomod_skip, "--fomod-skip"),
        (args.interactive, "--interactive"),
        (args.non_interactive, "--non-interactive"),
        (args.direct_markdown, "--direct-markdown"),
        (args.allow_processes, "--allow-processes"),
        (args.patcher.is_some(), "--patcher"),
        (args.patcher_engine.is_some(), "--patcher-engine"),
        (args.scratch_dir.is_some(), "--scratch-dir"),
        (args.max_expanded_bytes.is_some(), "--max-expanded-bytes"),
        (args.install_progress_json, "--install-progress-json"),
    ]
    .into_iter()
    .filter_map(|(supplied, flag)| supplied.then_some(flag))
    .collect::<Vec<_>>();
    ensure!(
        unsupported.is_empty(),
        "Basic validation without both game and source roots does not support {}; supply both --game-dir and --source-dir for execution settings",
        unsupported.join(", ")
    );
    let input = args
        .input
        .clone()
        .or_else(|| {
            profile
                .and_then(|profile| profile.instruction_file_path.as_deref())
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
        })
        .context("Supply --input or configure the profile's input path")?;
    conversion_cancelled(cancelled)?;
    let mut guide = load_validation_guide(&input, profile.is_some())?;
    if let Some(profile) = profile {
        profile.apply(&mut guide);
    } else if args.select.is_empty() && !args.use_file_selection {
        for component in &mut guide.components {
            component.is_selected = true;
        }
    }
    let detected =
        game.map(|game| modsync::game::detect(&modsync::game::resolve_install_root(game)));
    apply_selection(
        &mut guide.components,
        detected,
        &args.select,
        &args.option,
        &args.selection_flags,
        !report_mode && !args.errors_only,
    )?;
    require_validation_selection(&guide, args)?;
    let choices = args
        .fomod_choices
        .as_ref()
        .map(|path| modsync::fomod::ChoicesFile::load(path))
        .transpose()?;
    let prepared = modsync::phase::prepare(
        &guide,
        modsync::phase::Phase::All,
        modsync::phase::CompletionContext::None,
        choices.as_ref(),
    )?;
    conversion_cancelled(cancelled)?;
    let result = modsync::validation::basic::validate(&prepared.guide, cancelled);
    conversion_cancelled(cancelled)?;
    let mut report = result?;
    report.input_path = Some(input.to_string_lossy().into_owned());
    *report_emitted = true;
    present_validation_report(&report, args.errors_only, report_mode)
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

fn install_nexus_key(explicit: Option<&str>) -> Result<Option<String>> {
    if let Some(key) = explicit {
        ensure!(
            !key.trim().is_empty(),
            "An explicit Nexus API key cannot be blank"
        );
        return Ok(Some(key.to_owned()));
    }
    for name in ["KOTOR_MODSYNC_NEXUS_API_KEY", "NEXUS_MODS_API_KEY"] {
        match std::env::var(name) {
            Ok(key) if !key.trim().is_empty() => return Ok(Some(key)),
            Ok(_) | Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                anyhow::bail!("Nexus API key environment variable is not Unicode")
            }
        }
    }
    Ok(None)
}

fn conversion_cancelled(cancelled: &std::sync::atomic::AtomicBool) -> Result<()> {
    if cancelled.load(Ordering::Acquire) {
        return Err(cli_session::Exit(130).into());
    }
    Ok(())
}

fn process_conversion(
    guide: &mut Guide,
    options: &ConversionProcessing,
    input: Option<&std::path::Path>,
    cancelled: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    let result = process_conversion_inner(guide, options, input, cancelled);
    conversion_cancelled(cancelled)?;
    result
}

fn process_conversion_inner(
    guide: &mut Guide,
    options: &ConversionProcessing,
    input: Option<&std::path::Path>,
    cancelled: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    if let Some(path) = &options.spoiler_free {
        let text = modsync::editor::read_document(path)?;
        let overlay = modsync::spoiler::apply_markdown(guide, &text)?;
        ensure!(
            overlay.complete,
            "Spoiler overlay has unresolved mappings: {}",
            serde_json::to_string(&overlay.diagnostics)?
        );
        if !overlay.ignored_fields.is_empty() {
            eprintln!(
                "Spoiler overlay projection: {}",
                serde_json::to_string(&overlay.ignored_fields)?
            );
        }
        *guide = overlay.guide;
    }
    if options.download {
        let result = cli_acquisition::before_process(
            guide,
            options
                .source_path
                .as_deref()
                .context("--download requires --source-path")?,
            None,
            options.settings_dir.as_deref(),
            &modsync::archive::Options::default(),
            cancelled.clone(),
            &cli_acquisition::Options {
                guide_path: input.map(std::path::Path::to_owned),
                nexus_api_key: options.nexus_mods_api_key.clone(),
                workers: if options.concurrent { 4 } else { 1 },
                fomod_choices: options.fomod_choices.clone(),
                fomod_skip: options.fomod_skip,
                fomod_interactive: options.interactive,
                fomod_non_interactive: options.non_interactive,
                timeout: None,
                completed: Vec::new(),
                include_unselected_authoring: true,
            },
        );
        if cancelled.load(Ordering::Acquire) {
            return Err(cli_session::Exit(130).into());
        }
        result?;
    }
    if options.auto_generate {
        conversion_cancelled(cancelled)?;
        let needs_nexus = guide.components.iter().any(|component| {
            component.resource_registry.keys().any(|url| {
                matches!(
                    modsync::download_hosts::provider(url),
                    Ok(modsync::download_hosts::Provider::Nexus)
                )
            })
        });
        let key = cli_acquisition::resolve_nexus_key(
            options.nexus_mods_api_key.as_deref(),
            options.settings_dir.as_deref(),
            needs_nexus,
        )?;
        let resolved = modsync::download_orchestrator::filenames::resolve(guide, key, cancelled)?;
        conversion_cancelled(cancelled)?;
        ensure!(
            !resolved.cancelled && resolved.diagnostics.is_empty(),
            "Filename resolution is incomplete: {}",
            serde_json::to_string(&resolved.diagnostics)?
        );
        let reports = modsync::archive_generation::resolved::generate(
            guide,
            &resolved.resolved,
            options.source_path.as_deref(),
            &modsync::archive::Options::default(),
            cancelled,
        )?;
        conversion_cancelled(cancelled)?;
        eprintln!("{}", serde_json::to_string_pretty(&reports)?);
    }
    if options.auto_generate_local {
        conversion_cancelled(cancelled)?;
        let reports = modsync::archive_generation::generate_local_with_cancel(
            guide,
            options
                .source_path
                .as_deref()
                .context("Local generation requires --source-path")?,
            cancelled,
        );
        conversion_cancelled(cancelled)?;
        eprintln!("{}", serde_json::to_string_pretty(&reports?)?);
    }
    // Selection is an output projection after inference, as in the reference.
    // Omission preserves the existing conversion selection behavior.
    if !options.select.is_empty() {
        for warning in
            modsync::selection::apply_cli_selection(&mut guide.components, &options.select)?
        {
            eprintln!("Warning: {warning}");
        }
    }
    conversion_cancelled(cancelled)?;
    Ok(())
}

fn command_definition() -> clap::Command {
    Cli::command()
        .mut_subcommand("install", |command| {
            command
                .mut_arg("source_dir", |_| {
                    clap::Arg::new("source_dir")
                        .long("source-dir")
                        .short('s')
                        .help("Mod files directory; defaults to the input guide directory")
                        .value_parser(clap::value_parser!(PathBuf))
                })
                .mut_arg("game_dir", |argument| argument.short('g'))
        })
        .mut_subcommand("validate", |command| {
            // Missing validation roots are reported by the validation adapter, which
            // can preserve --output json's single error document. Install keeps its
            // parser-level required game root.
            command
                .mut_arg("game_dir", |_| {
                    clap::Arg::new("game_dir")
                        .long("game-dir")
                        .short('g')
                        .value_parser(clap::value_parser!(PathBuf))
                })
                .mut_arg("source_dir", |_| {
                    clap::Arg::new("source_dir")
                        .long("source-dir")
                        .short('s')
                        .value_parser(clap::value_parser!(PathBuf))
                })
        })
}

fn finish_acquisition_result<T>(result: Result<T>, cancelled: &AtomicBool) -> Result<T> {
    match result {
        // Published success wins over a signal that arrived after completion.
        Ok(value) => Ok(value),
        Err(_) if cancelled.load(Ordering::Acquire) => Err(cli_session::Exit(130).into()),
        Err(error) => Err(error),
    }
}

fn acquisition_scope(
    output: &std::path::Path,
    source: Option<&std::path::Path>,
    cancelled: &AtomicBool,
) -> Result<modsync::acquisition_guard::Scope> {
    finish_acquisition_result(
        modsync::acquisition_guard::Scope::acquire(&[output.to_path_buf()], source, cancelled),
        cancelled,
    )
}

fn run() -> Result<()> {
    let arguments = cli_compat::normalize_args(std::env::args_os().collect())?;
    let matches = command_definition().get_matches_from(arguments);
    let parsed = Cli::from_arg_matches(&matches)?;
    let command = parsed.command;
    if matches!(&command, None | Some(Action::Gui { .. })) || cli_session::transport_probe(&matches)
    {
        return dispatch(command, true);
    }
    let session = cli_session::Session::start(cli_session::settings_directory(&matches));
    if parsed.verbose {
        modsync::logging::set_debug_logging(true);
        modsync::logging::set_mirror_stderr(true);
    }
    // Progress is already plain text; the compatibility flag needs no renderer swap.
    let _ = parsed.plaintext;
    let result = dispatch(command, session.case_insensitive_pathing);
    if let Err(error) = &result
        && !error.is::<cli_session::Exit>()
    {
        modsync::logging::log_error(error, "CLI command stopped");
    }
    session.finish(result.is_ok());
    result
}

fn dispatch(command: Option<Action>, case_insensitive_pathing: bool) -> Result<()> {
    let (command, resume) = match command {
        Some(Action::Checkpoint(args)) => match args.into_resume() {
            Ok((install, resume)) => (Some(Action::Install(install)), Some(resume)),
            Err(args) => (Some(Action::Checkpoint(*args)), None),
        },
        command => (command, None),
    };
    // FOMOD mapper fallbacks are log warnings, as in C#; these verbs mirror the
    // log to stderr so they stay visible without touching machine stdout.
    if matches!(
        command,
        Some(
            Action::Validate(_)
                | Action::Install(_)
                | Action::FetchDownloads { .. }
                | Action::FomodInspect { .. }
        )
    ) {
        modsync::logging::set_mirror_stderr(true);
    }
    match command {
        Some(Action::Maintenance(args)) => args.run(),
        None => gui(None, None, None, None, None, false, GuiProbe::default()),
        Some(Action::Gui {
            settings_dir,
            input,
            game_dir,
            source_dir,
            protocol,
            allow_multiple_instances,
            nexus_mods_api_key,
            nexus_api_base,
            extra_ca_file,
        }) => gui(
            input,
            game_dir,
            source_dir,
            settings_dir,
            protocol,
            allow_multiple_instances,
            GuiProbe {
                nexus_mods_api_key,
                nexus_api_base,
                extra_ca_file,
            },
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
                    let store =
                        modsync::profiles::Store::from_settings_directory(settings_dir.as_deref())?;
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
        Some(Action::InstallPythonDeps { force }) => {
            let _ = force;
            anyhow::bail!(
                "install-python-deps targets the reference application's Python.Included runtime, which ModSync.rs does not own. Use a verified native HoloPatcher package or an externally managed Python environment with the backend's dependencies. No packages were installed."
            )
        }
        Some(Action::HoloPatcher {
            patcher,
            allow_processes,
            legacy_arguments,
            working_directory,
            timeout_seconds,
            arguments,
        }) => {
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let result = modsync::patcher::standalone::run(
                &modsync::patcher::standalone::Options {
                    executable: patcher,
                    arguments,
                    legacy_arguments,
                    working_directory,
                    allow_processes,
                    timeout: std::time::Duration::from_secs(timeout_seconds),
                },
                &cancelled,
            );
            conversion_cancelled(&cancelled)?;
            let output = result?;
            std::io::stdout().write_all(output.stdout.as_bytes())?;
            std::io::stdout().flush()?;
            std::io::stderr().write_all(output.stderr.as_bytes())?;
            std::io::stderr().flush()?;
            if output.exit_code == 0 {
                Ok(())
            } else {
                Err(cli_session::Exit(output.exit_code).into())
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
            if let Some(module) = &module {
                modsync::fomod::log_fallback_warnings(&module.config);
            }
            if let Some(path) = choices {
                let choices = modsync::fomod::ChoicesFile::load(&path)?;
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
        Some(Action::GuideDraftReview { input, component }) => {
            let guide = modsync::editor::read_guide(&input)?;
            let component = modsync::editor::find_component(&guide, &component)
                .context("Unknown generated component or option GUID")?;
            let diagnostics = modsync::draft::review::original_diagnostics(component)?;
            anyhow::ensure!(
                diagnostics
                    .unparsed_gaps
                    .len()
                    .saturating_add(diagnostics.conditional_drafts.len())
                    .saturating_add(diagnostics.rejected_drafts)
                    <= 4096,
                "Draft diagnostic count exceeds the review bound"
            );
            let operation = modsync::editor::EditOperation::ResolveDraft {
                component: component.guid.clone(),
                resolution: modsync::draft::review::Resolution {
                    expected_identity: modsync::draft::review::identity(component)?,
                    gap_decisions: vec![String::new(); diagnostics.unparsed_gaps.len()],
                    condition_decisions: vec![String::new(); diagnostics.conditional_drafts.len()],
                    rejected_decisions: vec![String::new(); diagnostics.rejected_drafts],
                    review_note: String::new(),
                },
            };
            println!(
                "{}",
                modsync::editor::render_preview(&serde_json::json!({
                    "diagnostics": diagnostics,
                    "requiresReview": modsync::draft::requires_review(component),
                    "operations": [operation]
                }))?
            );
            Ok(())
        }
        Some(Action::GuideEdit {
            input,
            operations,
            output,
        }) => {
            let guide = Guide::load_preserving_selection(&input)?;
            let operations = modsync::editor::load_operations(&operations)?;
            let edited = modsync::editor::apply_operations(&guide, &operations)?;
            modsync::editor::save_distinct(&edited, &input, &output)
        }
        Some(Action::GuideImportPreview {
            input,
            document,
            format,
            regex_profile,
            replace,
            parent,
            index,
        }) => {
            let guide = Guide::load_preserving_selection(&input)?;
            let mode = if let Some(guid) = replace {
                modsync::editor::ImportMode::Replace { guid }
            } else {
                let length = if let Some(parent) = &parent {
                    modsync::editor::find_component(&guide, parent)
                        .context("Unknown import parent GUID")?
                        .options
                        .len()
                } else {
                    guide.components.len()
                };
                modsync::editor::ImportMode::Append {
                    parent,
                    index: index.unwrap_or(length),
                }
            };
            let text = modsync::editor::read_document(&document)?;
            if let Some(profile) = regex_profile {
                let profile = serde_json::from_str(&modsync::editor::read_document(&profile)?)?;
                let preview = modsync::editor::preview_regex_import(&guide, &text, &profile, mode)?;
                println!("{}", modsync::editor::render_preview(&preview)?);
            } else {
                let preview = modsync::editor::preview_import(&guide, &text, &format, mode)?;
                println!("{}", modsync::editor::render_preview(&preview)?);
            }
            Ok(())
        }
        Some(Action::GuideImportApply {
            input,
            preview,
            output,
        }) => {
            let guide = Guide::load_preserving_selection(&input)?;
            let value: serde_json::Value =
                serde_json::from_str(&modsync::editor::read_document(&preview)?)?;
            let preview = if value.get("preview").is_some() {
                serde_json::from_value::<modsync::editor::RegexImportPreview>(value)?.preview
            } else {
                serde_json::from_value(value)?
            };
            let edited = modsync::editor::apply_preview(&guide, &preview)?;
            modsync::editor::save_distinct(&edited, &input, &output)
        }
        Some(Action::GuideResources {
            input,
            component,
            source_dir,
            game_dir,
            max_expanded_bytes,
        }) => {
            let guide = Guide::load_preserving_selection(&input)?;
            let component = modsync::editor::find_component(&guide, &component)
                .context("Unknown component GUID")?;
            let roots = Roots::new(&source_dir, &game_dir)?;
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let report = modsync::editor::resource_inventory(
                component,
                &roots,
                &modsync::archive::Options {
                    scratch_dir: None,
                    max_expanded_bytes: Some(max_expanded_bytes),
                },
                &cancelled,
            )?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            ensure!(
                report.resources.iter().all(|item| item.error.is_none()),
                "Some registered resources could not be inspected; see report"
            );
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
            let cancelled = modsync::p2p::stop_signal_flag()?;
            finish_acquisition_result(
                modsync::protocol::fetch_instruction_with_cancel(&url, &output, &cancelled),
                &cancelled,
            )?;
            println!("{}", output.display());
            Ok(())
        }
        Some(Action::ProtocolOpen { url }) => {
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let (link, guide) = finish_acquisition_result(
                modsync::protocol::fetch_modsync_guide_with_cancel(&url, &cancelled),
                &cancelled,
            )?;
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
            source_dir,
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
                        modsync::profiles::Store::from_settings_directory(settings_dir.as_deref())?
                            .nexus_api_key()?
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
                let report = orchestrator::run_with_source_context(
                    &plan,
                    &settings,
                    &guide,
                    &AtomicBool::new(false),
                    |_| {},
                    None,
                )?;
                let metadata = orchestrator::collect_metadata(
                    &plan,
                    &report,
                    &settings,
                    &AtomicBool::new(false),
                    None,
                );
                if record {
                    let summary = orchestrator::record(&mut guide, &report)?;
                    let metadata_recorded = orchestrator::record_metadata(&mut guide, &metadata)?;
                    if !summary.recorded.is_empty() || metadata_recorded > 0 {
                        guide.save(&input)?;
                    }
                }
                let mut output_report = serde_json::to_value(&report)?;
                output_report
                    .as_object_mut()
                    .expect("acquisition report is an object")
                    .insert("providerMetadata".into(), serde_json::to_value(&metadata)?);
                println!("{}", serde_json::to_string_pretty(&output_report)?);
                anyhow::ensure!(
                    report
                        .outcomes
                        .iter()
                        .all(|o| o.status == orchestrator::TargetStatus::Completed),
                    "The nxm download did not complete"
                );
                return Ok(());
            }
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let guard = acquisition_scope(&output, source_dir.as_deref(), &cancelled)?;
            let output = &guard.destinations()[0];
            let mut client = match nexus_api_base {
                Some(base) => modsync::updates::NexusClient::with_base_url(key, &base)?,
                None => modsync::updates::NexusClient::new(key)?,
            };
            let result = (|| -> Result<_> {
                let download_link = client.nxm_download_link_with_cancel(&link, &cancelled)?;
                guard.verify_destination(output)?;
                download::fetch(&download_link.uri, output, None, &cancelled)?;
                Ok(download_link)
            })();
            let download_link = finish_acquisition_result(result, &cancelled)?;
            println!("{}", serde_json::to_string_pretty(&download_link)?);
            Ok(())
        }
        Some(Action::UpdateSummary {
            input,
            settings_dir,
        }) => {
            let guide = Guide::load_preserving_selection(&input)?;
            let directory = modsync::profiles::settings_directory(settings_dir.as_deref())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&modsync::updates::load_snapshot(&directory, &guide))?
            );
            Ok(())
        }
        Some(Action::UpdateApply {
            input,
            versions,
            output,
            summary,
        }) => {
            let mut guide = Guide::load_preserving_selection(&input)?;
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
                None => modsync::profiles::Store::from_settings_directory(settings_dir.as_deref())?.nexus_api_key()?,
            }
                .or_else(|| std::env::var("KOTOR_MODSYNC_NEXUS_API_KEY").ok())
                .or_else(|| std::env::var("NEXUS_MODS_API_KEY").ok())
                .filter(|key| !key.trim().is_empty())
                .context("Set nexusModsApiKey in settings, an API-key flag, or a Nexus API-key environment variable")?;
            let mut guide = Guide::load_preserving_selection(&input)?;
            let mut client = modsync::updates::NexusClient::new(Some(key))?;
            let original = guide.clone();
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let outcome = modsync::updates::check_with_cancel(&mut guide, &mut client, &cancelled);
            if outcome.cancelled {
                println!("{}", serde_json::to_string_pretty(&outcome)?);
                return Err(cli_session::Exit(130).into());
            }
            let report = outcome.summary;
            let snapshot = modsync::updates::StoredSummary::new(&original, &guide, report.clone())?;
            let fresh = Guide::load_preserving_selection(&input)?;
            anyhow::ensure!(
                snapshot.may_apply_to(&fresh)?,
                "Guide changed during the update check; results were not saved"
            );
            match std::fs::symlink_metadata(&output) {
                Ok(metadata) => anyhow::ensure!(
                    metadata.file_type().is_file(),
                    "Update output must be a regular file"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            if cancelled.load(std::sync::atomic::Ordering::SeqCst) {
                let outcome = modsync::updates::CheckOutcome {
                    summary: report,
                    cancelled: true,
                };
                println!("{}", serde_json::to_string_pretty(&outcome)?);
                return Err(cli_session::Exit(130).into());
            }
            guide.save(&output)?;
            std::fs::create_dir_all(&directory)?;
            modsync::updates::save_summary(&directory, &report)?;
            snapshot.save(&directory)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            Ok(())
        }
        Some(Action::MergePreview {
            existing,
            incoming,
            options,
        }) => {
            let existing = modsync::editor::read_guide(&existing)?;
            let incoming = modsync::editor::read_guide(&incoming)?;
            let options = options
                .as_deref()
                .map(modsync::editor::read_document)
                .transpose()?
                .map(|text| serde_json::from_str::<modsync::merge::Options>(&text))
                .transpose()?
                .unwrap_or_default();
            let preview = modsync::merge::preview(&existing, &incoming, &options)?;
            println!("{}", modsync::editor::render_preview(&preview)?);
            Ok(())
        }
        Some(Action::MergeApply {
            existing,
            incoming,
            preview,
            output,
        }) => {
            let candidate: modsync::merge::Preview =
                serde_json::from_str(&modsync::editor::read_document(&preview)?)?;
            let existing_guide = modsync::editor::read_guide(&existing)?;
            let incoming_guide = modsync::editor::read_guide(&incoming)?;
            let merged =
                modsync::merge::apply_preview(&existing_guide, &incoming_guide, &candidate)?;
            let absolute_output = std::path::absolute(&output)?;
            let resolved_output = absolute_output
                .parent()
                .context("Merge output needs a parent")?
                .canonicalize()?
                .join(
                    absolute_output
                        .file_name()
                        .context("Merge output needs a filename")?,
                );
            ensure!(
                resolved_output != incoming.canonicalize()?,
                "Merge output must differ from both input guides"
            );
            modsync::editor::save_distinct(&merged, &existing, &output)
        }
        Some(Action::Merge {
            existing,
            incoming,
            output,
            existing_path,
            incoming_path,
            output_path,
            format,
            processing,
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
            let existing_path = existing_path
                .or(existing)
                .context("Supply --existing or an existing guide path")?;
            let incoming_path = incoming_path
                .or(incoming)
                .context("Supply --incoming or an incoming guide path")?;
            let existing = Guide::load(&existing_path)?;
            let mut incoming = Guide::load(&incoming_path)?;
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
            let cancelled = modsync::p2p::stop_signal_flag()?;
            process_conversion(&mut incoming, &processing, Some(&incoming_path), &cancelled)?;
            let output = output_path.or(output);
            let format = format.as_deref().unwrap_or("toml");
            conversion_cancelled(&cancelled)?;
            let rendered = incoming.render(format)?;
            conversion_cancelled(&cancelled)?;
            if let Some(output) = output {
                let output = if processing.download {
                    let resolved = modsync::editor::distinct_output_path(&existing_path, &output)?;
                    modsync::editor::distinct_output_path(&incoming_path, &resolved)?
                } else {
                    std::path::absolute(output)?
                };
                let destination = std::path::absolute(output)?;
                conversion_cancelled(&cancelled)?;
                recovery::atomic_write(&destination, rendered.as_bytes())?;
            } else {
                println!("{rendered}");
            }
            Ok(())
        }
        Some(Action::Convert {
            input,
            output,
            input_path,
            output_path,
            stdin,
            format,
            parse_directions,
            review_draft,
            ignore_errors,
            processing,
        }) => {
            let input_file = input_path.or(input);
            let (mut guide, markdown) = if stdin {
                use std::io::Read;
                let mut bytes = Vec::new();
                std::io::stdin()
                    .take(8 * 1024 * 1024 + 1)
                    .read_to_end(&mut bytes)?;
                ensure!(
                    bytes.len() <= 8 * 1024 * 1024,
                    "Conversion input exceeds 8 MiB"
                );
                // The qualified UTF-8 console path retains replacement characters;
                // file ingestion separately replaces them with underscores.
                let text = String::from_utf8_lossy(&bytes);
                (Guide::parse(&text, "auto")?, Guide::markdown_text(&text))
            } else {
                let path = input_file
                    .as_deref()
                    .context("Use --input <file> or --stdin")?;
                (Guide::load(path)?, Guide::markdown_input(path)?)
            };
            // C# convert writes components in its dependency resolver's order. Its Markdown
            // ingest drops InstallBefore/InstallAfter, so Markdown keeps file order there.
            if !markdown {
                for warning in guide.sort_like_dependency_resolver(ignore_errors)? {
                    eprintln!("{warning}");
                }
            }
            if parse_directions {
                modsync::draft::generate(&mut guide);
            }
            let cancelled = modsync::p2p::stop_signal_flag()?;
            process_conversion(&mut guide, &processing, input_file.as_deref(), &cancelled)?;
            conversion_cancelled(&cancelled)?;
            for id in review_draft {
                let component = guide
                    .components
                    .iter_mut()
                    .find(|c| c.guid.eq_ignore_ascii_case(&id))
                    .with_context(|| format!("Unknown component GUID: {id}"))?;
                modsync::draft::mark_reviewed(component)?;
            }
            let format = format.as_deref().unwrap_or("toml");
            conversion_cancelled(&cancelled)?;
            let text = guide.render(format)?;
            conversion_cancelled(&cancelled)?;
            if let Some(path) = output_path.as_ref().or(output.as_ref()) {
                if processing.download
                    && let Some(input) = &input_file
                {
                    // The editor boundary rejects canonical aliases of the source.
                    let destination = modsync::editor::distinct_output_path(input, path)?;
                    conversion_cancelled(&cancelled)?;
                    recovery::atomic_write(&destination, text.as_bytes())?;
                    return Ok(());
                }
                let destination = std::path::absolute(path)?;
                conversion_cancelled(&cancelled)?;
                recovery::atomic_write(&destination, text.as_bytes())?;
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
        Some(Action::Protocol(args)) => args.run(),
        Some(Action::SetNexusApiKey(args)) => args.run(),
        Some(Action::Checkpoint(args)) => args.run(),
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
            deadlystream_base,
            mega_api_base,
            extra_ca_file,
            peer_mode,
            scratch_dir,
            max_expanded_bytes,
            dry_run,
            progress_json,
            record,
            fomod_skip,
            fomod_choices,
            interactive,
            non_interactive,
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
                // A DeadlyStream or MEGA fixture run reads no stored settings and starts no peers.
                _ if deadlystream_base.is_some() || mega_api_base.is_some() => {
                    (p2p::PeerMode::Off, None)
                }
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
            let keep_finished_sharing = peer_settings.as_ref().is_some_and(|settings| {
                settings.sharing && settings.keep_sharing && settings.mode != p2p::PeerMode::Off
            });
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
                    // A DeadlyStream or MEGA fixture run reads no stored credential.
                    None if needs_nexus
                        && deadlystream_base.is_none()
                        && mega_api_base.is_none() =>
                    {
                        modsync::profiles::Store::from_settings_directory(settings_dir.as_deref())?
                            .nexus_api_key()?
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
            let cancelled = if dry_run {
                Arc::new(AtomicBool::new(false))
            } else {
                p2p::stop_signal_flag()?
            };
            // A Ctrl+C during planning (present-file hashing) exits 130 like one during the run.
            let plan = finish_acquisition_result(
                orchestrator::plan_with_cancel(&execution, &candidates, &plan_options, &cancelled),
                &cancelled,
            )?;

            let loopback_ca = extra_ca_file
                .as_deref()
                .map(modsync::download::LoopbackCa::from_pem_file)
                .transpose()?;
            if deadlystream_base.is_some() {
                anyhow::ensure!(
                    loopback_ca.is_some(),
                    "--deadlystream-base requires --extra-ca-file"
                );
            }
            if mega_api_base.is_some() {
                anyhow::ensure!(
                    loopback_ca.is_some(),
                    "--mega-api-base requires --extra-ca-file"
                );
            }

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
                deadlystream_base,
                mega_api_base,
                peer,
            };
            // Execution only: dry-run returned above without network discovery. A
            // discovery error after a signal exits 130, as planning and the run do.
            let plan = finish_acquisition_result(
                orchestrator::discover_with_loopback_ca(
                    &execution,
                    &plan,
                    &plan_options,
                    &run_settings,
                    &cancelled,
                    loopback_ca.as_ref(),
                ),
                &cancelled,
            )?;
            let stdout = std::sync::Mutex::new(std::io::stdout());
            let report = orchestrator::run_with_source_context(
                &plan,
                &run_settings,
                &execution,
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

            let metadata = orchestrator::collect_metadata(
                &plan,
                &report,
                &run_settings,
                &cancelled,
                loopback_ca.as_ref(),
            );

            let scan = orchestrator::collect_fomod_consumers(
                &execution,
                &report,
                &archive_options,
                &cancelled,
                &[],
            );
            let inspection_errors = scan.inspection_errors.clone();
            let mut configuration = modsync::fomod_cli::dispatch(
                &mut execution,
                scan,
                &modsync::fomod_cli::Options {
                    skip: fomod_skip,
                    choices: fomod_choices,
                    interactive,
                    non_interactive,
                    settings_dir: settings_dir.clone(),
                },
                &cancelled,
                &[],
            );
            // Follow-ups describe the post-dispatch state: consumers still lacking choices.
            let fomod = orchestrator::FomodFollowups {
                followups: configuration.followups(),
                inspection_errors,
            };
            // Recording uses the pristine authored guide so transient selection is
            // never persisted. Commit accepted sessions only, after dispatch; a
            // cancelled current session has no edit to replay.
            let mut fomod_recorded = false;
            let record_summary = if record {
                match (|| -> Result<orchestrator::RecordSummary> {
                    let summary = orchestrator::record(&mut guide, &report)?;
                    let metadata_recorded = orchestrator::record_metadata(&mut guide, &metadata)?;
                    // FOMOD replay is all-or-nothing; on failure the guide still holds the
                    // acquisition facts, which are saved alone while the error fails the run.
                    let applied = match configuration.apply_recorded(&mut guide, &[]) {
                        Ok(()) => configuration.has_edits(),
                        Err(error) => {
                            configuration
                                .errors
                                .push(format!("Recording FOMOD configuration: {error:#}"));
                            false
                        }
                    };
                    if !summary.recorded.is_empty() || applied || metadata_recorded > 0 {
                        guide.save(&input)?;
                        fomod_recorded = applied;
                    }
                    Ok(summary)
                })() {
                    Ok(summary) => Some(summary),
                    Err(error) => {
                        configuration
                            .errors
                            .push(format!("Recording Fetch result: {error:#}"));
                        None
                    }
                }
            } else {
                None
            };

            // Latch classification after recording/cleanup so the emitted summary
            // and exit code describe the same completed run boundary.
            configuration.cancelled = report.cancelled || cancelled.load(Ordering::SeqCst);
            let mut sharing_enrollment = modsync::peer_sharing::Enrollment::default();
            if keep_finished_sharing && !configuration.cancelled {
                sharing_enrollment =
                    match modsync::profiles::SettingsFile::resolve(settings_dir.as_deref())
                        .and_then(|settings| modsync::peer_sharing::Catalog::open(&settings))
                    {
                        Ok(catalog) => modsync::peer_sharing::enroll_report(
                            &catalog,
                            &execution,
                            &plan,
                            &report,
                            &archive_options,
                            Some(input.as_path()),
                            true,
                        ),
                        Err(error) => modsync::peer_sharing::Enrollment {
                            failures: vec![format!("Sharing enrollment storage: {error:#}")],
                            ..Default::default()
                        },
                    };
            }
            let summary_value = serde_json::json!({
                "report": report,
                "recordSummary": record_summary,
                "providerMetadata": metadata,
                "fomodFollowups": fomod,
                "fomodConfiguration": configuration,
                "fomodConfigurationRecorded": fomod_recorded,
                "planWarnings": plan.warnings,
                "sharingEnrollment": {
                    "admitted": sharing_enrollment.admitted,
                    "duplicates": sharing_enrollment.duplicates,
                    "rejected": sharing_enrollment.rejected,
                    "failures": sharing_enrollment.failures,
                },
            });
            if progress_json {
                let envelope = serde_json::json!({"type": "summary", "summary": summary_value});
                println!("{}", serde_json::to_string(&envelope)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&summary_value)?);
            }

            if configuration.cancelled {
                return Err(cli_session::Exit(130).into());
            }
            let failed = report
                .outcomes
                .iter()
                .any(|o| o.status == orchestrator::TargetStatus::Failed);
            // Sharing enrollment failures are reported in the summary only; they
            // never turn a completed acquisition into a failed command.
            if failed || !fomod.inspection_errors.is_empty() || configuration.has_errors() {
                return Err(cli_session::Exit(1).into());
            }
            let manual = report
                .outcomes
                .iter()
                .any(|o| o.status == orchestrator::TargetStatus::ManualRequired);
            if manual || configuration.unresolved > 0 {
                return Err(cli_session::Exit(3).into());
            }
            Ok(())
        }
        Some(Action::Download {
            url,
            output,
            source_dir,
            sha256,
            max_expanded_bytes,
        }) => {
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let guard = acquisition_scope(&output, source_dir.as_deref(), &cancelled)?;
            finish_acquisition_result(
                download::fetch_with_options(
                    &url,
                    &guard.destinations()[0],
                    sha256.as_deref(),
                    &cancelled,
                    &modsync::archive::Options {
                        max_expanded_bytes,
                        ..Default::default()
                    },
                ),
                &cancelled,
            )
        }
        Some(Action::TorrentDownload {
            magnet,
            output,
            source_dir,
            sha256,
            peer,
            max_expanded_bytes,
            torrent_info,
            proxy,
        }) => {
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let guard = acquisition_scope(&output, source_dir.as_deref(), &cancelled)?;
            finish_acquisition_result(
                modsync::p2p::cli_download_with_cancel(
                    &magnet,
                    torrent_info.as_deref(),
                    &guard.destinations()[0],
                    &sha256,
                    peer,
                    &proxy.into(),
                    &modsync::archive::Options {
                        max_expanded_bytes,
                        ..Default::default()
                    },
                    &cancelled,
                ),
                &cancelled,
            )
        }
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
            source_dir,
            workers,
            retries,
            scratch_dir,
            max_expanded_bytes,
        }) => {
            let cancelled = modsync::p2p::stop_signal_flag()?;
            let jobs = finish_acquisition_result(
                modsync::download_queue::load_jobs(&input, &cancelled),
                &cancelled,
            )?;
            let results = finish_acquisition_result(
                modsync::download_queue::fetch_all_in_source_root(
                    jobs,
                    source_dir.as_deref(),
                    workers,
                    retries,
                    &cancelled,
                    modsync::archive::Options {
                        scratch_dir,
                        max_expanded_bytes,
                    },
                ),
                &cancelled,
            )?;
            println!("{}", serde_json::to_string_pretty(&results)?);
            if results
                .iter()
                .any(|item| item.status == modsync::download_queue::Status::Cancelled)
            {
                return Err(cli_session::Exit(130).into());
            }
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
            let component_checkpoints = args.component_checkpoints || resume.is_some();
            let pipeline_mode = args.validation_report
                || args.validation_output.is_some()
                || args.errors_only
                || args.full
                || args.dry_run
                || args.dry_run_only;
            let mut report_mode = (args.validation_report
                || args.validation_output == Some(ValidationOutput::Json))
                && !install
                && !conflicts;
            let validation_json = args.json && !install && !conflicts;
            let mut validation_started = false;
            let mut validation_emitted = false;
            let dispatch_cancelled = modsync::p2p::stop_signal_flag()?;
            let dispatch = (|| -> Result<()> {
                let refused = [
                    (args.skip_validation, "--skip-validation"),
                    (
                        args.continue_on_missing_sources,
                        "--continue-on-missing-sources",
                    ),
                    (args.continue_on_mod_failure, "--continue-on-mod-failure"),
                    (args.ignore_errors, "--ignore-errors"),
                    (args.yes, "--yes"),
                ]
                .into_iter()
                .filter_map(|(supplied, flag)| supplied.then_some(flag))
                .collect::<Vec<_>>();
                ensure!(
                    refused.is_empty(),
                    "{} would skip validation, auto-confirm, or continue past a missing source, failed mod or dependency error. ModSync always validates and stops on failure: fix the reported cause, or restore the verified boundary and resume the ordered suffix.",
                    refused.join(", ")
                );
                ensure!(
                    (!install && !conflicts) || !validation_only_flags(&args),
                    "Validation report/output/full/dry-run/error-filter options are only supported by validate"
                );
                ensure!(
                    args.phase == PhaseArg::All || !args.direct_markdown,
                    "Phased Direct Markdown is unsupported: separate-invocation definitions are not stable"
                );
                ensure!(
                    args.phase != PhaseArg::Remaining || !args.dry_run_only,
                    "Remaining reports always run full validation and dry run; --dry-run-only is incompatible"
                );
                let profile = if args.profile.is_some() || args.active_profile {
                    let store = modsync::profiles::Store::from_settings_directory(
                        args.settings_dir.as_deref(),
                    )?;
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
                let basic_validation = !install
                    && !conflicts
                    && args.phase == PhaseArg::All
                    && !args.full
                    && !args.dry_run
                    && !args.dry_run_only;
                if basic_validation {
                    let game = args.game_dir.clone().or_else(|| {
                        profile
                            .as_ref()
                            .and_then(|profile| profile.kotor_directory.as_deref())
                            .filter(|path| !path.is_empty())
                            .map(PathBuf::from)
                    });
                    let source = args.source_dir.clone().or_else(|| {
                        profile
                            .as_ref()
                            .and_then(|profile| profile.mod_directory.as_deref())
                            .filter(|path| !path.is_empty())
                            .map(PathBuf::from)
                    });
                    if game.is_none() || source.is_none() {
                        // The reference's basic mode ignores even supplied roots.
                        // Complete roots retain this CLI's existing engine extension;
                        // missing roots use the guide-only path without fake folders.
                        // Rootless --json reports the same single document as
                        // --output json; complete roots retain their event stream.
                        report_mode |= args.json;
                        return validate_without_roots(
                            &args,
                            profile.as_ref(),
                            game.as_deref(),
                            report_mode,
                            &dispatch_cancelled,
                            &mut validation_emitted,
                        );
                    }
                }
                ensure!(
                    install || !args.download,
                    "--download is only supported by install, convert and merge"
                );
                ensure!(
                    !args.download || resume.is_none(),
                    "Checkpoint resume requires acquisition to be completed separately before reviewing the resume boundary"
                );
                ensure!(
                    !args.install_progress_json || install,
                    "--install-progress-json is only supported by install"
                );
                ensure!(
                    install
                        || !(args.component_checkpoints || args.no_checkpoint || args.best_effort),
                    "Checkpoint and failure-continuation flags are only supported by install"
                );
                ensure!(
                    !args.best_effort,
                    "--best-effort/--continue-on-error cannot preserve guide order after a failure. Restore the verified boundary before the failed component, fix its cause, then resume the ordered suffix. Installation always stops on failure."
                );
                ensure!(
                    !args.no_checkpoint || resume.is_none(),
                    "Checkpoint resume requires retained component history"
                );
                ensure!(
                    !args.managed || (install && component_checkpoints),
                    "--managed requires install --component-checkpoints or checkpoint resume"
                );
                // Preserve implicit-directory provenance, including legacy fallback.
                // Reading preferences does not discover or launch a backend.
                let patcher_preferences = Some(
                    modsync::profiles::Store::from_settings_directory(
                        args.settings_dir.as_deref(),
                    )?
                    .patcher_preferences()?,
                );
                let patcher_engine = args
                    .patcher_engine
                    .map(Into::into)
                    .or_else(|| {
                        patcher_preferences
                            .as_ref()
                            .map(|preferences| preferences.engine)
                    })
                    .unwrap_or_default();
                let paths = if let Some(profile) = &profile {
                    profile.install_paths(
                        args.input.as_deref(),
                        args.game_dir.as_deref(),
                        args.source_dir.as_deref(),
                    )?
                } else {
                    let input = args.input.clone().context("Supply --input")?;
                    let source = match &args.source_dir {
                        Some(source) => source.clone(),
                        None if install => std::path::absolute(&input)?
                            .parent()
                            .context("Input guide has no parent directory")?
                            .to_owned(),
                        None => anyhow::bail!("Supply --source-dir"),
                    };
                    modsync::profiles::InstallPaths {
                        input,
                        game: args.game_dir.clone().context("Supply --game-dir")?,
                        source,
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
                let mut guide = load_validation_guide(&paths.input, profile.is_some())?;
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
                // Ordinary validation rejects an empty effective selection before
                // pipeline construction, so JSON uses the setup ErrorReport shape.
                if !install && !conflicts && args.phase == PhaseArg::All {
                    require_validation_selection(&guide, &args)?;
                }
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
                    .map(|path| modsync::fomod::ChoicesFile::load(path))
                    .transpose()?;
                let context = if args.phase == PhaseArg::Remaining {
                    modsync::phase::CompletionContext::Reconstruct
                } else {
                    modsync::phase::CompletionContext::None
                };
                let mut settings = Settings::new(&paths.source, &paths.game)?;
                settings.cancelled = dispatch_cancelled.clone();
                let prepared = modsync::phase::prepare_with_roots(
                    &guide,
                    args.phase.into(),
                    context,
                    choices.as_ref(),
                    &settings.roots,
                    case_insensitive_pathing,
                    &settings.cancelled,
                )?;
                let mut guide = prepared.guide;
                settings.allow_processes = args.allow_processes;
                settings.patcher = args.patcher.or_else(|| {
                    patcher_preferences
                        .filter(|preferences| preferences.engine == patcher_engine)
                        .and_then(|preferences| preferences.executable.map(PathBuf::from))
                });
                settings.patcher_engine = patcher_engine;
                settings.archive.scratch_dir = args.scratch_dir;
                settings.archive.max_expanded_bytes = args.max_expanded_bytes;
                if args.download {
                    cli_acquisition::before_process(
                        &mut guide,
                        &paths.source,
                        Some(&paths.game),
                        args.settings_dir.as_deref(),
                        &settings.archive,
                        settings.cancelled.clone(),
                        &cli_acquisition::Options {
                            guide_path: Some(paths.input.clone()),
                            nexus_api_key: args.nexus_mods_api_key.clone(),
                            workers: if args.concurrent { 4 } else { 1 },
                            timeout: Some(std::time::Duration::from_secs(
                                u64::from(if args.download_timeout_hours == 0 {
                                    48
                                } else {
                                    args.download_timeout_hours
                                }) * 3600,
                            )),
                            fomod_choices: args.fomod_choices.clone(),
                            fomod_skip: args.fomod_skip,
                            fomod_interactive: args.interactive,
                            fomod_non_interactive: args.non_interactive,
                            completed: prepared.completed.clone(),
                            include_unselected_authoring: false,
                        },
                    )?;
                }
                let mut engine =
                    Engine::new_with_case_insensitive_pathing(settings, case_insensitive_pathing)?
                        .with_component_checkpoints(component_checkpoints);
                if args.managed {
                    let deployment = modsync::deployment::Service::resolve_readonly(
                        &paths.game,
                        args.staging_dir.as_deref(),
                        args.manifest_dir.as_deref(),
                        args.settings_dir.as_deref(),
                        // A guide profile may still supply selection and install
                        // paths when explicit deployment roots override its artifacts.
                        if args.staging_dir.is_some() {
                            None
                        } else {
                            profile.as_ref().map(|profile| profile.name.as_str())
                        },
                    )?;
                    engine = engine.with_managed_deployment(deployment)?;
                }
                if let Some(resume) = &resume {
                    let roots = modsync::paths::Roots::new(
                        &paths.source,
                        &modsync::game::resolve_install_root(&paths.game),
                    )?;
                    let current =
                        modsync::checkpoint::Store::open(&roots)?.inspect(&resume.session, None)?;
                    engine = engine.with_checkpoint_resume(modsync::checkpoint::ResumeRequest {
                        expected: modsync::checkpoint::ExpectedHead {
                            session_id: current.session_id,
                            checkpoint_id: current.head_checkpoint_id,
                            generation: current.generation,
                        },
                        accept_source_changes: resume.accept_source_changes,
                    })?;
                }
                let emit = |event: modsync::Event| {
                    if pipeline_mode {
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
                    let progress = |progress: modsync::install_progress::Progress| {
                        if args.install_progress_json {
                            let envelope = serde_json::json!({
                                "type": "installProgress",
                                "progress": progress,
                            });
                            if let Ok(mut line) = serde_json::to_vec(&envelope) {
                                line.push(b'\n');
                                let _ = std::io::stdout().lock().write_all(&line);
                            }
                        }
                    };
                    let report = if args.phase == PhaseArg::Remaining {
                        engine.install_remaining_observed_report(
                            &guide,
                            &prepared.completed,
                            emit,
                            progress,
                        )?
                    } else {
                        engine.install_observed_report(&guide, emit, progress)?
                    };
                    if args.managed || component_checkpoints {
                        if args.json || args.install_progress_json {
                            println!(
                                "{}",
                                serde_json::to_string(&serde_json::json!({
                                    "type": "installResult", "report": report,
                                }))?
                            );
                        } else {
                            println!("Install result: {}", serde_json::to_string_pretty(&report)?);
                        }
                    }
                    Ok(())
                } else if pipeline_mode {
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
                    validation_emitted = true;
                    present_validation_report(&report, args.errors_only, report_mode)
                } else if args.phase == PhaseArg::Remaining {
                    validation_started = true;
                    engine.validate_remaining(&guide, &prepared.completed, emit)
                } else {
                    validation_started = true;
                    engine.validate(&guide, emit)
                }
            })();
            // A completed installation and its receipt win over a late signal.
            if dispatch_cancelled.load(Ordering::Acquire) && (!install || dispatch.is_err()) {
                if (report_mode || (validation_json && !validation_started)) && !validation_emitted
                {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&modsync::validation::ErrorReport::new(
                            "Validation cancelled".into()
                        ))?
                    );
                } else if !(validation_emitted || validation_json && validation_started) {
                    eprintln!(
                        "{} cancelled",
                        if install {
                            "Installation"
                        } else if conflicts {
                            "Conflict analysis"
                        } else {
                            "Validation"
                        }
                    );
                }
                return Err(cli_session::Exit(130).into());
            }
            if report_mode || (validation_json && !validation_started) {
                if let Err(error) = dispatch {
                    if error.is::<cli_session::Exit>() {
                        return Err(error);
                    }
                    // Setup failed before any pipeline stage ran; emit the C#
                    // SerializeError shape as the sole machine output.
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&modsync::validation::ErrorReport::new(
                            format!("{error:#}")
                        ))?
                    );
                    return Err(cli_session::Exit(1).into());
                }
                Ok(())
            } else {
                dispatch
            }
        }
    }
}

/// Hidden desktop probe options; any of them makes a probe launch.
#[derive(Default)]
struct GuiProbe {
    nexus_mods_api_key: Option<String>,
    nexus_api_base: Option<String>,
    extra_ca_file: Option<PathBuf>,
}

fn gui(
    input: Option<PathBuf>,
    game: Option<PathBuf>,
    mods: Option<PathBuf>,
    settings_dir: Option<PathBuf>,
    protocol: Option<String>,
    allow_multiple_instances: bool,
    probe: GuiProbe,
) -> Result<()> {
    #[cfg(feature = "desktop")]
    {
        let probe = (probe.nexus_mods_api_key.is_some()
            || probe.nexus_api_base.is_some()
            || probe.extra_ca_file.is_some())
        .then_some(ui::Probe {
            nexus_api_key: probe.nexus_mods_api_key,
            nexus_api_base: probe.nexus_api_base,
            extra_ca_file: probe.extra_ca_file,
        });
        ui::launch(
            input,
            game,
            mods,
            settings_dir,
            protocol,
            allow_multiple_instances,
            probe,
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
            probe.nexus_mods_api_key,
            probe.nexus_api_base,
            probe.extra_ca_file,
        );
        anyhow::bail!(
            "Desktop support is disabled. Build with the default features, or use --help for CLI commands."
        )
    }
}

fn main() {
    if let Err(error) = run() {
        if let Some(exit) = error.downcast_ref::<cli_session::Exit>() {
            std::process::exit(exit.0);
        }
        eprintln!("Error: {error:#}");
        std::process::exit(1);
    }
}
