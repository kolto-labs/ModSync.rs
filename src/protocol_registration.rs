use anyhow::{Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const FILE_NAME: &str = "modsync-protocol.desktop";
const MIMES: [&str; 2] = ["x-scheme-handler/modsync", "x-scheme-handler/nxm"];

pub fn windows_reg_commands(executable: &Path) -> Result<[String; 4]> {
    scheme_windows_reg_commands(executable, "modsync", "ModSync Protocol")
}

pub fn windows_nxm_reg_commands(executable: &Path) -> Result<[String; 4]> {
    scheme_windows_reg_commands(executable, "nxm", "Nexus Mods Protocol")
}

fn scheme_windows_reg_commands(
    executable: &Path,
    scheme: &str,
    description: &str,
) -> Result<[String; 4]> {
    if executable.as_os_str().is_empty() {
        bail!("Executable path is required");
    }
    let executable = executable.display();
    Ok([
        format!("add HKCU\\Software\\Classes\\{scheme} /ve /d \"URL:{description}\" /f"),
        format!("add HKCU\\Software\\Classes\\{scheme} /v \"URL Protocol\" /d \"\" /f"),
        format!(
            "add HKCU\\Software\\Classes\\{scheme}\\DefaultIcon /ve /d \"\\\"{executable}\\\",0\" /f"
        ),
        format!(
            "add HKCU\\Software\\Classes\\{scheme}\\shell\\open\\command /ve /d \"\\\"{executable}\\\" \\\"%1\\\"\" /f"
        ),
    ])
}

pub fn macos_url_types_plist_fragment() -> &'static str {
    "<key>CFBundleURLTypes</key>\n<array>\n  <dict>\n    <key>CFBundleURLName</key>\n    <string>ModSync Protocol</string>\n    <key>CFBundleURLSchemes</key>\n    <array>\n      <string>modsync</string>\n    </array>\n  </dict>\n</array>"
}

pub fn macos_nxm_url_types_plist_fragment() -> &'static str {
    "<key>CFBundleURLTypes</key>\n<array>\n  <dict>\n    <key>CFBundleURLName</key>\n    <string>Nexus Mods Protocol</string>\n    <key>CFBundleURLSchemes</key>\n    <array>\n      <string>nxm</string>\n    </array>\n  </dict>\n</array>"
}

pub fn linux_desktop_entry(executable: &Path) -> Result<String> {
    if executable.as_os_str().is_empty() {
        bail!("Executable path is required");
    }
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=ModSync\nComment=ModSync protocol handler for build and Nexus links\nExec=\"{}\" gui --protocol %u\nTerminal=false\nNoDisplay=true\nMimeType={};\n",
        executable.display(),
        MIMES.join(";")
    ))
}

pub fn register_linux(executable: &Path, applications_dir: &Path) -> Result<PathBuf> {
    let path = write_linux_entry(executable, applications_dir)?;
    for mime in MIMES {
        let _ = Command::new("xdg-mime")
            .args(["default", FILE_NAME, mime])
            .status();
    }
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
