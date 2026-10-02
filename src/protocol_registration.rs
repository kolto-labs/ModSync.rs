use anyhow::{Context, Result, bail, ensure};
pub mod owned;
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILE_NAME: &str = "modsync-protocol.desktop";
const MIMES: [&str; 2] = ["x-scheme-handler/modsync", "x-scheme-handler/nxm"];

#[derive(Clone, Debug)]
pub struct PackageIdentity {
    pub package_token: String,
    pub channel: String,
}

/// Resolve the owning package namespace. Only an absent package manifest uses
/// the development identity; an unreadable or malformed manifest is an error.
pub fn package_identity(executable: &Path) -> Result<PackageIdentity> {
    use std::io::Read;
    let executable = executable
        .canonicalize()
        .context("Resolve protocol executable")?;
    let base = executable
        .parent()
        .context("Protocol executable needs a parent")?;
    let manifest = if base.file_name().is_some_and(|name| name == "MacOS")
        && base
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == "Contents")
    {
        base.parent()
            .unwrap()
            .join("Resources/modsync-package.json")
    } else {
        base.join("modsync-package.json")
    };
    match fs::symlink_metadata(&manifest) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PackageIdentity {
                package_token: "org.modsync.rust".into(),
                channel: "stable".into(),
            });
        }
        Err(error) => return Err(error.into()),
        Ok(_) => (),
    }
    let mut bytes = Vec::new();
    crate::paths::open_regular_read(&manifest)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 4 * 1024 * 1024,
        "Protocol package manifest exceeds 4 MiB"
    );
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).context("Read protocol package identity")?;
    ensure!(
        value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            == Some(1),
        "Unsupported protocol package manifest version"
    );
    let field = |name| -> Result<String> {
        let value = value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .context("Package registration identity is missing")?;
        ensure!(
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte)),
            "Invalid package registration identity"
        );
        Ok(value.into())
    };
    Ok(PackageIdentity {
        package_token: field("registrationToken")?,
        channel: field("channel")?,
    })
}

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
            "add HKCU\\Software\\Classes\\{scheme}\\shell\\open\\command /ve /d \"\\\"{executable}\\\" gui --protocol \\\"%1\\\"\" /f"
        ),
    ])
}

pub fn macos_url_types_plist_fragment() -> &'static str {
    "<key>CFBundleURLTypes</key>\n<array>\n  <dict>\n    <key>CFBundleURLName</key>\n    <string>ModSync Protocol</string>\n    <key>CFBundleURLSchemes</key>\n    <array>\n      <string>modsync</string>\n    </array>\n  </dict>\n</array>"
}

pub fn macos_nxm_url_types_plist_fragment() -> &'static str {
    "<key>CFBundleURLTypes</key>\n<array>\n  <dict>\n    <key>CFBundleURLName</key>\n    <string>Nexus Mods Protocol</string>\n    <key>CFBundleURLSchemes</key>\n    <array>\n      <string>nxm</string>\n    </array>\n  </dict>\n</array>"
}

/// One bundle declaration for both schemes. Concatenating the legacy single-
/// scheme fragments would duplicate CFBundleURLTypes and lose one registration.
pub fn macos_combined_url_types_plist_fragment() -> &'static str {
    "<key>CFBundleURLTypes</key>\n<array>\n  <dict>\n    <key>CFBundleURLName</key>\n    <string>ModSync links</string>\n    <key>CFBundleURLSchemes</key>\n    <array><string>modsync</string><string>nxm</string></array>\n  </dict>\n</array>"
}

pub fn linux_desktop_entry(executable: &Path) -> Result<String> {
    if executable.as_os_str().is_empty() {
        bail!("Executable path is required");
    }
    let executable = executable
        .to_str()
        .context("Protocol executable path is not UTF-8")?;
    ensure!(
        !executable.contains(['\n', '\r', '\0']),
        "Protocol executable contains a control separator"
    );
    // Desktop Entry parsing and Exec quoting each consume an escape layer.
    let executable = executable
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%");
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=ModSync\nComment=ModSync protocol handler for build and Nexus links\nExec=\"{}\" gui --protocol %u\nTerminal=false\nNoDisplay=true\nMimeType={};\n",
        executable,
        MIMES.join(";")
    ))
}

pub fn register_linux(executable: &Path, applications_dir: &Path) -> Result<PathBuf> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (executable, applications_dir);
        bail!("Linux registration is available only on Linux")
    }
    #[cfg(target_os = "linux")]
    {
        let directory = dirs::data_local_dir()
            .context("Cannot locate per-user applications")?
            .join("applications");
        ensure!(
            std::path::absolute(applications_dir)? == std::path::absolute(&directory)?,
            "Registration must use the per-user XDG applications directory; use write_linux_entry to export an entry elsewhere"
        );
        let identity = package_identity(executable)?;
        let report = owned::register(
            &owned::Request {
                executable: executable.into(),
                package_token: identity.package_token,
                channel: identity.channel,
                schemes: vec![owned::Scheme::Modsync, owned::Scheme::Nxm],
                replace: false,
            },
            &std::sync::atomic::AtomicBool::new(false),
        )?;
        let handler = report
            .entries
            .iter()
            .find(|entry| entry.scheme == owned::Scheme::Modsync)
            .and_then(|entry| entry.handler.as_ref())
            .context("Registered handler missing from receipt")?;
        Ok(directory.join(handler))
    }
}

pub fn write_linux_entry(executable: &Path, applications_dir: &Path) -> Result<PathBuf> {
    let entry = linux_desktop_entry(executable)?;
    fs::create_dir_all(applications_dir)?;
    let path = applications_dir.join(FILE_NAME);
    crate::recovery::atomic_write(&std::path::absolute(&path)?, entry.as_bytes())?;
    Ok(path)
}
