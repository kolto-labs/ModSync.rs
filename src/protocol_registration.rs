use anyhow::{Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FILE_NAME: &str = "modsync-protocol.desktop";
const MIME: &str = "x-scheme-handler/modsync";

pub fn linux_desktop_entry(executable: &Path) -> Result<String> {
    if executable.as_os_str().is_empty() {
        bail!("Executable path is required");
    }
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=ModSync\nComment=ModSync protocol handler for Install with ModSync build links\nExec=\"{}\" gui --protocol %u\nTerminal=false\nNoDisplay=true\nMimeType={MIME};\n",
        executable.display()
    ))
}

pub fn register_linux(executable: &Path, applications_dir: &Path) -> Result<PathBuf> {
    let path = write_linux_entry(executable, applications_dir)?;
    let _ = Command::new("xdg-mime")
        .args(["default", FILE_NAME, MIME])
        .status();
    let _ = Command::new("update-desktop-database")
        .arg(applications_dir)
        .status();
    Ok(path)
}

pub fn write_linux_entry(executable: &Path, applications_dir: &Path) -> Result<PathBuf> {
    let entry = linux_desktop_entry(executable)?;
    fs::create_dir_all(applications_dir)?;
    let path = applications_dir.join(FILE_NAME);
    fs::write(&path, entry)?;
    Ok(path)
}
