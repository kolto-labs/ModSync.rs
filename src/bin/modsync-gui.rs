//! Windows GUI-subsystem entry point; the console companion owns all application logic.
#![cfg_attr(windows, windows_subsystem = "windows")]
use std::process::Command;

fn run() -> anyhow::Result<i32> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Launcher has no directory"))?;
    let companion = directory.join(if cfg!(windows) {
        "modsync.exe"
    } else {
        "modsync"
    });
    anyhow::ensure!(
        companion.is_file(),
        "ModSync console companion is missing: {}",
        companion.display()
    );
    let mut arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_none_or(|argument| argument != "gui") {
        arguments.insert(0, "gui".into());
    }
    let mut command = Command::new(companion);
    command.args(arguments);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW for the console companion.
    }
    Ok(command.status()?.code().unwrap_or(1))
}
fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            #[cfg(windows)]
            {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    MB_ICONERROR, MB_OK, MessageBoxW,
                };
                let message: Vec<_> = format!("ModSync could not start: {error:#}")
                    .encode_utf16()
                    .chain(Some(0))
                    .collect();
                let title: Vec<_> = "ModSync".encode_utf16().chain(Some(0)).collect();
                unsafe {
                    MessageBoxW(
                        std::ptr::null_mut(),
                        message.as_ptr(),
                        title.as_ptr(),
                        MB_OK | MB_ICONERROR,
                    );
                }
            }
            #[cfg(not(windows))]
            eprintln!("ModSync could not start: {error:#}");
            std::process::exit(1);
        }
    }
}
