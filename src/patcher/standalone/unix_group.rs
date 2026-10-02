//! Read-only process-group membership while the unreaped leader reserves its ID.
use anyhow::{Context, Result, ensure};
fn vanished(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::ESRCH | libc::ENOENT))
}
fn member(pid: i32, group: i32) -> Result<bool> {
    let actual = unsafe { libc::getpgid(pid) };
    if actual >= 0 {
        return Ok(actual == group);
    }
    let error = std::io::Error::last_os_error();
    if vanished(&error) {
        return Ok(false);
    }
    Err(error).context("Inspect process group membership")
}
#[cfg(target_os = "linux")]
pub(super) fn live(group: i32) -> Result<bool> {
    use std::io::Read;
    for (index, entry) in std::fs::read_dir("/proc")?.enumerate() {
        ensure!(index < 1_000_000, "Process inventory exceeds bounds");
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
        else {
            continue;
        };
        if !member(pid, group)? {
            continue;
        }
        // A zombie main thread may still have live sibling threads. Inspect the
        // complete task directory, not just the thread-group leader's stat.
        let tasks = match std::fs::read_dir(entry.path().join("task")) {
            Ok(tasks) => tasks,
            Err(error) if vanished(&error) => {
                ensure!(
                    !member(pid, group)?,
                    "Process remains in the owned group but its task directory is unavailable"
                );
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        let mut tasks_seen = 0usize;
        for (index, task) in tasks.enumerate() {
            ensure!(index < 1_000_000, "Thread inventory exceeds bounds");
            let task = task?;
            let mut file = match std::fs::File::open(task.path().join("stat")) {
                Ok(file) => file,
                Err(error) if vanished(&error) => continue,
                Err(error) => return Err(error.into()),
            };
            let mut bytes = Vec::new();
            if let Err(error) = (&mut file).take(8193).read_to_end(&mut bytes) {
                if vanished(&error) {
                    continue;
                }
                return Err(error.into());
            }
            ensure!(bytes.len() <= 8192, "Process stat exceeds bounds");
            // comm is parenthesized and can contain ')' or non-UTF8 bytes.
            let end = bytes
                .iter()
                .rposition(|byte| *byte == b')')
                .context("Malformed process stat")?;
            let rest = std::str::from_utf8(&bytes[end + 1..])?;
            let mut fields = rest.split_ascii_whitespace();
            let state = fields.next().context("Missing process state")?;
            fields.next().context("Missing process parent")?;
            let actual: i32 = fields.next().context("Missing process group")?.parse()?;
            tasks_seen += 1;
            if actual == group && !matches!(state, "Z" | "X" | "x") {
                return Ok(true);
            }
        }
        ensure!(
            tasks_seen > 0 || !member(pid, group)?,
            "Process remains in the owned group but has no observable task inventory"
        );
    }
    Ok(false)
}
#[cfg(target_os = "macos")]
pub(super) fn live(group: i32) -> Result<bool> {
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    ensure!(
        count > 0 && count < 1_000_000,
        "Cannot size process inventory"
    );
    let mut pids = vec![0i32; count as usize + 128];
    let returned = unsafe {
        libc::proc_listallpids(
            pids.as_mut_ptr().cast(),
            std::mem::size_of_val(pids.as_slice()) as i32,
        )
    };
    ensure!(
        returned > 0 && (returned as usize) < pids.len(),
        "Process inventory failed or changed during enumeration"
    );
    for pid in pids
        .into_iter()
        .take(returned as usize)
        .filter(|pid| *pid > 0)
    {
        if !member(pid, group)? {
            continue;
        }
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info) as i32;
        let actual = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        if actual != size {
            let error = std::io::Error::last_os_error();
            if vanished(&error) {
                continue;
            }
            anyhow::bail!("Cannot inspect process state: {error}");
        }
        if info.pbi_pgid == group as u32 && info.pbi_status != libc::SZOMB {
            return Ok(true);
        }
    }
    Ok(false)
}
