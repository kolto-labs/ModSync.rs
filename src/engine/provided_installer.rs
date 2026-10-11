//! Explicit supplied-installer execution. Original installers have no invented CLI.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{BufRead, Read};

#[derive(Clone, Debug)]
pub struct WindowsProcessRunner {
    pub executable: PathBuf,
    pub wineserver: PathBuf,
    pub prefix: PathBuf,
}
#[derive(Clone, Debug)]
pub struct ProvidedInstallerRunner {
    pub request_dir: PathBuf,
    pub xvfb: PathBuf,
    pub windows: Option<WindowsProcessRunner>,
}
impl ProvidedInstallerRunner {
    pub(super) fn command(&self, executable: &Path, display: &str) -> Result<Command> {
        ensure!(
            cfg!(target_os = "linux"),
            "Supplied GUI installer runner currently requires Linux headless X11"
        );
        let windows_exe = executable
            .extension()
            .and_then(|part| part.to_str())
            .is_some_and(|part| part.eq_ignore_ascii_case("exe"));
        let mut command = if windows_exe {
            let runner = self
                .windows
                .as_ref()
                .context("Windows installer requires an explicitly configured Wine runner")?;
            let program = fs::canonicalize(&runner.executable)?;
            crate::paths::open_regular_read(&program)?;
            ensure!(
                program.is_absolute(),
                "Wine executable must have an absolute path"
            );
            let prefix = fs::canonicalize(&runner.prefix)?;
            let requests = fs::canonicalize(&self.request_dir)?;
            ensure!(
                prefix.is_dir() && prefix.starts_with(&requests) && prefix != requests,
                "Wine prefix must be a dedicated directory inside installer request storage"
            );
            let mut command = Command::new(program);
            let server = fs::canonicalize(&runner.wineserver)?;
            crate::paths::open_regular_read(&server)?;
            command
                .env("WINEPREFIX", prefix)
                .env("WINESERVER", server)
                .env_remove("WINELOADER")
                .arg(executable);
            command
        } else {
            Command::new(executable)
        };
        command
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("SESSION_MANAGER")
            .env_remove("XAUTHORITY")
            .env("DISPLAY", display)
            .env("QT_QPA_PLATFORM", "xcb")
            .env("QT_QUICK_BACKEND", "software")
            .env("LIBGL_ALWAYS_SOFTWARE", "1");
        Ok(command)
    }
}

pub(super) struct HeadlessServer {
    child: std::process::Child,
    pub(super) display: String,
}
impl HeadlessServer {
    pub(super) fn start(executable: &Path) -> Result<Self> {
        ensure!(cfg!(target_os = "linux"), "Managed Xvfb requires Linux");
        let executable = fs::canonicalize(executable)?;
        crate::paths::open_regular_read(&executable)?;
        ensure!(
            executable.file_name().is_some_and(|name| name == "Xvfb"),
            "Configure the installed Xvfb server executable"
        );
        let mut child = Command::new(executable)
            .args([
                "-displayfd",
                "1",
                "-screen",
                "0",
                "1280x900x24",
                "-nolisten",
                "tcp",
                "-ac",
            ])
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("XAUTHORITY")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let output = child
            .stdout
            .take()
            .context("Xvfb display output unavailable")?;
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = std::io::BufReader::new(output)
                .take(64)
                .read_line(&mut line)
                .map(|_| line);
            let _ = sender.send(result);
        });
        let mut server = Self {
            child,
            display: String::new(),
        };
        let line = receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .context("Owned headless server did not allocate a display")??;
        let number = line
            .trim()
            .parse::<u32>()
            .context("Owned headless server returned an invalid display")?;
        ensure!(
            server.child.try_wait()?.is_none(),
            "Owned headless server exited before installer launch"
        );
        server.display = format!(":{number}");
        Ok(server)
    }
}
impl Drop for HeadlessServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct WinePrefixGuard {
    server: PathBuf,
    prefix: PathBuf,
    pending: PathBuf,
    done: bool,
}
impl WinePrefixGuard {
    fn server_command(&self, argument: &str, timeout: u64) -> Result<()> {
        let mut command = Command::new(&self.server);
        command
            .arg(argument)
            .env("WINEPREFIX", &self.prefix)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .env_remove("SESSION_MANAGER")
            .env_remove("XAUTHORITY");
        // Cleanup must finish even when the application cancellation flag is set.
        let cancel = AtomicBool::new(false);
        let receipt = crate::patcher::standalone::run_command(
            command,
            std::time::Duration::from_secs(timeout),
            &cancel,
        )?;
        ensure!(
            !receipt.cancelled && receipt.failure.is_none() && receipt.exit_code == Some(0),
            "Dedicated Wine prefix quiescence is unproven; keep checkpoint pending and do not restore while writers may remain"
        );
        Ok(())
    }
    fn prepare(runner: &WindowsProcessRunner, requests: &Path) -> Result<Self> {
        let prefix = fs::canonicalize(&runner.prefix)?;
        ensure!(
            prefix.is_dir() && prefix.starts_with(requests) && prefix != requests,
            "Wine prefix must be dedicated beneath request storage"
        );
        let server = fs::canonicalize(&runner.wineserver)?;
        crate::paths::open_regular_read(&server)?;
        let pending = requests.join("wine-prefix-active.json");
        ensure!(
            !pending.try_exists()?,
            "Wine prefix has an unresolved prior process; establish quiescence before recovery or retry"
        );
        let mut guard = Self {
            server,
            prefix,
            pending,
            done: true,
        };
        // Wait without killing any pre-existing server; refuse a busy prefix.
        guard.server_command("-w", 10)?;
        recovery::atomic_write(
            &guard.pending,
            &serde_json::to_vec_pretty(&serde_json::json!({
                "schemaVersion": 1, "status": "mayHaveLiveWriters", "winePrefix": guard.prefix,
                "wineserver": guard.server, "ownerPid": std::process::id()
            }))?,
        )?;
        fs::File::open(requests)?.sync_all()?;
        guard.done = false;
        Ok(guard)
    }
    fn finish(&mut self) -> Result<()> {
        if self.done {
            return Ok(());
        }
        self.server_command("-k", 30)?;
        self.server_command("-w", 30)?;
        fs::remove_file(&self.pending)?;
        fs::File::open(self.pending.parent().context("Wine marker has no parent")?)?.sync_all()?;
        self.done = true;
        Ok(())
    }
}
impl Drop for WinePrefixGuard {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.finish();
        }
    }
}
/// Shared across supplied Patcher and every configured Execute, so one prefix
/// cannot be reused concurrently with a different display or game root.
pub(super) struct RunnerSession {
    pub(super) headless: HeadlessServer,
    prefix: Option<WinePrefixGuard>,
    lock: fs::File,
    gate: crate::checkpoint::external_writer::Gate,
    submitted: bool,
}
impl RunnerSession {
    pub(super) fn start(
        runner: &ProvidedInstallerRunner,
        executable: &Path,
        roots: &Roots,
    ) -> Result<Self> {
        let requests = fs::canonicalize(&runner.request_dir)?;
        ensure!(
            !requests.join("wine-prefix-active.json").try_exists()?,
            "Runner has unresolved Wine writers; establish quiescence before recovery or retry"
        );
        let lock_path = requests.join("installer.lock");
        if let Ok(metadata) = fs::symlink_metadata(&lock_path) {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "Installer lock must be an ordinary file"
            );
        }
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("Another installer is using this runner/prefix")?;
        ensure!(
            !requests.join("wine-prefix-active.json").try_exists()?,
            "Runner has unresolved Wine writers"
        );
        let windows_exe = executable
            .extension()
            .and_then(|part| part.to_str())
            .is_some_and(|part| part.eq_ignore_ascii_case("exe"));
        let prefix = if windows_exe {
            Some(WinePrefixGuard::prepare(
                runner
                    .windows
                    .as_ref()
                    .context("Windows execution requires explicit Wine and wineserver")?,
                &requests,
            )?)
        } else {
            None
        };
        let headless = HeadlessServer::start(&runner.xvfb)?;
        let gate = crate::checkpoint::external_writer::Gate::register(
            roots,
            &requests,
            prefix.as_ref().map(|value| value.prefix.as_path()),
        )?;
        Ok(Self {
            headless,
            prefix,
            lock,
            gate,
            submitted: false,
        })
    }
    pub(super) fn mark_submitted(&mut self) {
        self.submitted = true;
    }
    pub(super) fn finish(&mut self, native_process_quiescent: bool) -> Result<()> {
        if let Some(prefix) = &mut self.prefix {
            prefix.finish()?;
        } else {
            ensure!(
                !self.submitted || native_process_quiescent,
                "Native installer quiescence is unproven; keep writer fence and checkpoint pending"
            );
        }
        self.gate.clear_after_quiescence()?;
        self.lock.sync_all()?;
        Ok(())
    }
}

impl Drop for RunnerSession {
    fn drop(&mut self) {
        if self.prefix.is_some() || !self.submitted {
            let _ = self.finish(false);
        }
    }
}
fn digest(path: &Path) -> Result<String> {
    let mut input = crate::paths::open_regular_read(path)?;
    let mut digest = Sha256::new();
    let mut chunk = [0u8; 65536];
    loop {
        let bytes = input.read(&mut chunk)?;
        if bytes == 0 {
            break;
        }
        digest.update(&chunk[..bytes]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
impl Engine {
    pub(super) fn process_supplied_installer(
        &mut self,
        instruction: &Instruction,
        source: &Path,
        emit: &mut impl FnMut(Event),
    ) -> Result<()> {
        ensure!(
            !self.exact_patcher_prediction,
            "Supplied installer effects are unknown; apply on disk and use its verified checkpoint before predicting beyond it"
        );
        if !self.install {
            return Ok(());
        }
        ensure!(
            self.settings.allow_processes,
            "Supplied installer requires explicit external-process trust"
        );
        let runner = self
            .settings
            .provided_installer
            .as_ref()
            .context("Missing supplied installer runner")?;
        ensure!(
            self.files.contains_key(&self.path_policy.key(source)),
            "Supplied installer Source must identify its actual executable file"
        );
        let base = source.parent().context("Installer has no parent")?;
        let destination = if instruction.destination.is_empty() {
            self.settings.roots.game.clone()
        } else {
            self.actual(&self.path(&instruction.destination, false)?)
        };
        ensure!(
            destination == self.settings.roots.game,
            "Supplied installer target must be the configured game root"
        );
        let requests = fs::canonicalize(&runner.request_dir)?;
        ensure!(
            !requests.starts_with(&self.settings.roots.mods)
                && !requests.starts_with(&self.settings.roots.game),
            "Installer request storage must be outside mod and game trees"
        );
        let (_stage, staged_base) = patcher_prepare::stage(base, &self.settings.cancelled)?;
        let staged_executable = staged_base.join(
            source
                .file_name()
                .context("Installer executable has no filename")?,
        );
        let source_hash = digest(source)?;
        ensure!(
            digest(&staged_executable)? == source_hash,
            "Supplied executable changed during staging"
        );
        let namespace = if instruction.arguments.trim().is_empty() {
            None
        } else {
            instruction.arguments.trim().parse::<usize>().ok()
        };
        let mut ini = None;
        let data = self.actual(&base.join("tslpatchdata"));
        if namespace.is_none() && !instruction.arguments.trim().is_empty() {
            let name = relative(instruction.arguments.trim(), false)?;
            ensure!(
                name.components().count() == 1
                    && name
                        .extension()
                        .and_then(|part| part.to_str())
                        .is_some_and(|part| part.eq_ignore_ascii_case("ini")),
                "Supplied patcher Arguments must be a namespace index or one INI filename"
            );
            ini = Some(name);
        }
        let variants = if data.join("namespaces.ini").is_file() {
            odypatcher::patcher::reader::NamespaceReader::from_filepath(
                &data.join("namespaces.ini"),
            )
            .map_err(|error| anyhow::anyhow!("Read supplied installer namespace: {error}"))?
        } else {
            Vec::new()
        };
        ensure!(
            variants.len() <= 1024,
            "Supplied installer exceeds namespace option bound"
        );
        let namespace_name = namespace
            .map(|index| -> Result<String> {
                if variants.is_empty() {
                    ensure!(index == 0, "Supplied installer has no namespace options");
                    Ok(String::new())
                } else {
                    Ok(variants
                        .get(index)
                        .context("Supplied namespace is out of range")?
                        .name
                        .clone())
                }
            })
            .transpose()?;
        if let Some(name) = &ini {
            let selected = self.actual(&data.join(name));
            let staged_data = staged_base.join(data.strip_prefix(base)?);
            recovery::atomic_copy(
                &staged_data.join("changes.ini"),
                crate::paths::open_regular_read(&staged_base.join(selected.strip_prefix(base)?))?,
                &self.settings.cancelled,
            )?;
            for item in fs::read_dir(&staged_data)? {
                let item = item?;
                if item.file_name().eq_ignore_ascii_case("namespaces.ini") {
                    fs::remove_file(item.path())?;
                }
            }
        }
        clear_patcher_logs(&staged_base)?;
        let checkpoints =
            crate::checkpoint::Store::open(&self.settings.roots)?.pending_operations()?;
        ensure!(
            !checkpoints.is_empty(),
            "Supplied installer execution requires an active durable checkpoint operation"
        );
        let mut session = RunnerSession::start(runner, &staged_executable, &self.settings.roots)?;
        let headless = &session.headless;
        let request_id = uuid::Uuid::new_v4().to_string();
        let request_path = requests.join(format!("{request_id}.json"));
        let mut request = serde_json::json!({
            "schemaVersion": 1, "requestId": request_id, "status": "awaitingHeadlessInteraction",
            "sourceExecutable": source, "stagedExecutable": staged_executable, "executableSha256": source_hash,
            "gameTarget": destination, "namespaceIndex": namespace, "namespaceName": namespace_name,
            "iniName": ini, "headlessDisplay": headless.display, "headlessServerPid": headless.child.id(), "checkpointOperations": checkpoints,
            "argumentsPassedToExecutable": [], "checkpointCommitVerified": false, "laneParityVerified": false
        });
        let mut command = runner.command(&staged_executable, &headless.display)?;
        command.current_dir(&staged_base);
        recovery::atomic_write(&request_path, &serde_json::to_vec_pretty(&request)?)?;
        fs::File::open(&requests)?.sync_all()?;
        emit(Event {
            component: instruction.action.clone(),
            step: 0,
            message: format!("Supplied installer request: {}", request_path.display()),
        });
        session.mark_submitted();
        let result = self.run_external_command(command);
        let quiescence = session.finish(
            result
                .as_ref()
                .is_ok_and(|receipt| receipt.no_child_started || receipt.process_group_quiescent),
        );
        let verification = (|| -> Result<()> {
            quiescence?;
            let receipt = result?;
            request["processReceipt"] = serde_json::json!({ "exitCode": receipt.exit_code, "cancelled": receipt.cancelled, "failure": receipt.failure, "processGroupQuiescent": receipt.process_group_quiescent, "noChildStarted": receipt.no_child_started, "stdout": receipt.stdout, "stderr": receipt.stderr });
            for text in [&receipt.stdout, &receipt.stderr] {
                if !text.is_empty() {
                    emit(Event {
                        component: instruction.action.clone(),
                        step: 0,
                        message: text.clone(),
                    });
                }
            }
            ensure!(
                !receipt.cancelled,
                "Supplied installer cancelled; restore before retrying"
            );
            ensure!(
                receipt.failure.is_none(),
                "Supplied installer process failure: {:?}; restore before retrying",
                receipt.failure
            );
            ensure!(
                receipt.exit_code == Some(0),
                "Supplied installer exited {:?}; restore before retrying",
                receipt.exit_code
            );
            let (log, content) = patcher_install_log(&staged_base)?;
            request["installLog"] = serde_json::json!({ "path": log, "sha256": format!("{:x}", Sha256::digest(content.as_bytes())), "text": content });
            emit(Event {
                component: instruction.action.clone(),
                step: 0,
                message: format!("Supplied installer log {}:\n{content}", log.display()),
            });
            ensure!(
                patcher_log_error_lines(&content).is_empty(),
                "Supplied installer log reports errors; restore before retrying"
            );
            Ok(())
        })();
        request["status"] = if verification.is_ok() {
            "processAndLogVerified".into()
        } else {
            "failedRestoreRequired".into()
        };
        if let Err(error) = &verification {
            request["error"] = format!("{error:#}").into();
        }
        recovery::atomic_write(&request_path, &serde_json::to_vec_pretty(&request)?)?;
        verification
    }
}
