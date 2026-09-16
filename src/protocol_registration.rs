use anyhow::{Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FILE_NAME: &str = "modsync-protocol.desktop";
const MIME: &str = "x-scheme-handler/modsync";

pub fn windows_reg_commands(executable: &Path) -> Result<[String; 4]> {
    if executable.as_os_str().is_empty() {
        bail!("Executable path is required");
    }
    let executable = executable.display();
    Ok([
        "add HKCU\\Software\\Classes\\modsync /ve /d \"URL:ModSync Protocol\" /f".into(),
        "add HKCU\\Software\\Classes\\modsync /v \"URL Protocol\" /d \"\" /f".into(),
        format!(
            "add HKCU\\Software\\Classes\\modsync\\DefaultIcon /ve /d \"\\\"{executable}\\\",0\" /f"
        ),
        format!(
            "add HKCU\\Software\\Classes\\modsync\\shell\\open\\command /ve /d \"\\\"{executable}\\\" \\\"%1\\\"\" /f"
        ),
    ])
}

pub fn macos_url_types_plist_fragment() -> &'static str {
    "<key>CFBundleURLTypes</key>\n<array>\n  <dict>\n    <key>CFBundleURLName</key>\n    <string>ModSync Protocol</string>\n    <key>CFBundleURLSchemes</key>\n    <array>\n      <string>modsync</string>\n    </array>\n  </dict>\n</array>"
}

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
