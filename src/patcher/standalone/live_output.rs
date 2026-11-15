//! File-backed external output forwarding without changing child file offsets.
use anyhow::{Context, Result, ensure};
use std::{fs::File, io::Write};

pub(super) const STREAM_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

pub(super) struct LiveOutput<'a> {
    files: [&'a File; 2],
    offsets: [u64; 2],
}

impl<'a> LiveOutput<'a> {
    pub(super) fn new(stdout: &'a File, stderr: &'a File) -> Self {
        Self {
            files: [stdout, stderr],
            offsets: [0, 0],
        }
    }

    /// Positional reads preserve the shared descriptors' child write cursors.
    /// Both streams go to parent stderr so structured CLI stdout stays usable.
    pub(super) fn drain(&mut self) -> Result<()> {
        use std::os::unix::fs::FileExt;
        let mut destination = std::io::stderr().lock();
        let mut buffer = [0u8; 64 * 1024];
        for (file, offset) in self.files.iter().zip(&mut self.offsets) {
            // A bounded quantum lets cancellation, timeout and writer ownership
            // be observed even while a backend continually emits output.
            for _ in 0..16 {
                let size = file.read_at(&mut buffer, *offset)?;
                if size == 0 {
                    break;
                }
                destination.write_all(&buffer[..size])?;
                *offset = offset
                    .checked_add(size as u64)
                    .context("External output size overflow")?;
                ensure!(
                    *offset <= STREAM_LIMIT,
                    "External output exceeds explicit 4 GiB per-stream budget"
                );
            }
        }
        destination.flush()?;
        Ok(())
    }

    pub(super) fn finish(&mut self) -> Result<()> {
        loop {
            let previous = self.offsets;
            self.drain()?;
            if self.offsets == previous {
                return Ok(());
            }
        }
    }
}

pub(super) fn command(command: &std::process::Command) -> Result<()> {
    let mut arguments = vec![command.get_program().to_string_lossy().into_owned()];
    arguments.extend(
        command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned()),
    );
    let cwd = command
        .get_current_dir()
        .map(std::path::Path::to_path_buf)
        .map_or_else(std::env::current_dir, Ok)?;
    // No environment values are enumerated. CI's parent stream redactor covers
    // credentials in authored argv before any output reaches a public runner.
    eprintln!(
        "external command cwd={} $ {}",
        shlex::try_quote(&cwd.to_string_lossy())?,
        shlex::try_join(arguments.iter().map(String::as_str))?
    );
    Ok(())
}
