mod profile_cli;
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand, ValueEnum};
use modsync::{Engine, Guide, PatcherEngine, Settings, download, paths::Roots, recovery};
use std::{ffi::OsString, io::Write, path::PathBuf, sync::atomic::AtomicBool};

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
        output: PathBuf,
        #[arg(long)]
        nexus_mods_api_key: Option<String>,
        #[arg(long)]
        settings_dir: Option<PathBuf>,
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
        prefer_existing_instructions: bool,
        #[arg(long)]
        prefer_existing_options: bool,
        #[arg(long)]
        prefer_existing_resource_registry: bool,
        #[arg(long)]
        exclude_existing_only: bool,
        #[arg(long)]
        exclude_incoming_only: bool,
    },
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
}
#[derive(Args)]
struct DeploymentArgs {
    #[command(subcommand)]
    action: DeploymentAction,
}
#[derive(Subcommand)]
enum DeploymentAction {
    /// List deployed component manifests, oldest first.
    Status {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        staging_dir: PathBuf,
        #[arg(long)]
        manifest_dir: PathBuf,
    },
    /// Deploy staging-dir/<component> into the game directory.
    Deploy {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        staging_dir: PathBuf,
        #[arg(long)]
        manifest_dir: PathBuf,
        #[arg(long)]
        component: String,
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove files owned by one manifest and restore its displaced files.
    Uninstall {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        staging_dir: PathBuf,
        #[arg(long)]
        manifest_dir: PathBuf,
        #[arg(long)]
        component: String,
    },
    /// Uninstall all manifests newest first.
    Purge {
        #[arg(long)]
        game_dir: PathBuf,
        #[arg(long)]
        staging_dir: PathBuf,
        #[arg(long)]
        manifest_dir: PathBuf,
    },
}
#[derive(Args)]
struct InstallArgs {
    /// Run all selected components, the base, or the verified remaining suffix.
    #[arg(long, value_enum, default_value = "all")]
    phase: PhaseArg,
    /// Install a Markdown guide in source order after direct-Markdown preflight.
    #[arg(long, conflicts_with_all = ["fomod_choices", "profile", "active_profile", "use_file_selection", "select", "option"])]
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
    /// Select option GUIDs (repeat the flag). Existing option selections are retained.
    #[arg(long)]
    option: Vec<String>,
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
    /// Emit newline-delimited typed installation progress.
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
            let (game, staging, manifests) = match &args.action {
                DeploymentAction::Status {
                    game_dir,
                    staging_dir,
                    manifest_dir,
                }
                | DeploymentAction::Deploy {
                    game_dir,
                    staging_dir,
                    manifest_dir,
                    ..
                }
                | DeploymentAction::Uninstall {
                    game_dir,
                    staging_dir,
                    manifest_dir,
                    ..
                }
                | DeploymentAction::Purge {
                    game_dir,
                    staging_dir,
                    manifest_dir,
                } => (game_dir, staging_dir, manifest_dir),
            };
            let service = modsync::deployment::Service::new(game, staging, manifests)?;
            match args.action {
                DeploymentAction::Status { .. } => {
                    println!("{}", serde_json::to_string_pretty(&service.deployed()?)?)
                }
                DeploymentAction::Deploy {
                    component, name, ..
                } => println!(
                    "{}",
                    serde_json::to_string_pretty(&service.deploy(&component, name)?)?
                ),
                DeploymentAction::Uninstall { component, .. } => println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"removed": service.uninstall(&component)?})
                    )?
                ),
                DeploymentAction::Purge { .. } => println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"purged": service.purge()?}))?
                ),
            }
            Ok(())
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
            nexus_mods_api_key,
            settings_dir,
        }) => {
            let link = modsync::protocol::parse(&url)?;
            anyhow::ensure!(
                matches!(link, modsync::protocol::Link::Nxm { .. }),
                "Expected an nxm:// URL"
            );
            let directory = modsync::profiles::settings_directory(settings_dir.as_deref())?;
            let key = nexus_mods_api_key
                .or(modsync::profiles::Store::new(&directory)?.nexus_api_key()?)
                .or_else(|| std::env::var("KOTOR_MODSYNC_NEXUS_API_KEY").ok())
                .or_else(|| std::env::var("NEXUS_MODS_API_KEY").ok());
            let mut client = modsync::updates::NexusClient::new(key)?;
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
            let key = nexus_mods_api_key
                .or(modsync::profiles::Store::new(&directory)?.nexus_api_key()?)
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
            prefer_existing_instructions,
            prefer_existing_options,
            prefer_existing_resource_registry,
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
        Some(Action::Profile(args)) => args.run(),
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
                if !args.select.is_empty() {
                    for warning in modsync::selection::apply_cli_selection(
                        &mut guide.components,
                        &args.select,
                    )? {
                        if !report_mode || args.phase == PhaseArg::All {
                            eprintln!("Selection warning: {warning}");
                        }
                    }
                } else {
                    for c in &mut guide.components {
                        if !args.use_file_selection && profile.is_none() {
                            c.is_selected = true;
                        }
                    }
                }
                fn select_option(c: &mut modsync::Component, id: &str) -> bool {
                    for o in &mut c.options {
                        if o.guid.eq_ignore_ascii_case(id) {
                            o.is_selected = true;
                            return true;
                        }
                        if select_option(o, id) {
                            return true;
                        }
                    }
                    false
                }
                for id in &args.option {
                    ensure!(
                        guide.components.iter_mut().any(|c| select_option(c, id)),
                        "Unknown option: {id}"
                    );
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
