//! Local-only named pipes, with a current-user DACL and peer SID/session checks.
use super::*;
use anyhow::Context;
use std::{
    ffi::c_void,
    io,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::mpsc,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions},
    time::{Instant, sleep, timeout},
};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY, HANDLE, LocalFree},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        },
        GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenSessionId,
        TokenUser,
    },
    System::{
        Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

#[derive(PartialEq, Eq)]
struct Identity {
    sid: String,
    session: u32,
}
fn identity(process: HANDLE) -> Result<Identity> {
    let mut token = std::ptr::null_mut();
    ensure!(
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } != 0,
        "Open pipe peer token: {}",
        io::Error::last_os_error()
    );
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut needed = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    ensure!(
        needed > 0 && needed <= 64 * 1024,
        "Invalid token identity length"
    );
    // usize storage gives TOKEN_USER and its SID the required alignment.
    let mut storage = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
    ensure!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                storage.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } != 0,
        "Read peer user identity"
    );
    let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = std::ptr::null_mut();
    ensure!(
        unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) } != 0,
        "Render peer SID"
    );
    let mut length = 0;
    while length < 256 && unsafe { *sid.add(length) } != 0 {
        length += 1;
    }
    let text = if length < 256 {
        String::from_utf16(unsafe { std::slice::from_raw_parts(sid, length) })
            .map_err(anyhow::Error::from)
    } else {
        Err(anyhow::anyhow!("SID string exceeds bound"))
    };
    unsafe {
        LocalFree(sid.cast());
    }
    let sid = text?;
    let mut session = 0u32;
    ensure!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenSessionId,
                (&mut session as *mut u32).cast(),
                4,
                &mut needed,
            )
        } != 0,
        "Read peer session identity"
    );
    Ok(Identity { sid, session })
}
fn peer(handle: HANDLE, client: bool, expected: &Identity) -> Result<()> {
    let mut pid = 0;
    let result = unsafe {
        if client {
            GetNamedPipeClientProcessId(handle, &mut pid)
        } else {
            GetNamedPipeServerProcessId(handle, &mut pid)
        }
    };
    ensure!(result != 0, "Cannot identify named-pipe peer");
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    ensure!(!process.is_null(), "Cannot authenticate named-pipe peer");
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    ensure!(
        identity(process.as_raw_handle())? == *expected,
        "Named-pipe peer belongs to another user or session"
    );
    Ok(())
}
fn server(name: &str, owner: &Identity, first: bool) -> Result<NamedPipeServer> {
    let sddl: Vec<u16> = format!("D:P(A;;GA;;;{})", owner.sid)
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut descriptor = std::ptr::null_mut();
    ensure!(
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } != 0,
        "Create private pipe security descriptor"
    );
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let result = unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(2)
            .in_buffer_size(FRAME_LIMIT as u32)
            .out_buffer_size(FRAME_LIMIT as u32)
            .create_with_security_attributes_raw(
                name,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast::<c_void>(),
            )
    };
    unsafe {
        LocalFree(descriptor);
    }
    Ok(result?)
}

pub(super) fn claim_or_forward(
    protocol: Option<&str>,
    namespace: String,
) -> Result<Option<(Receiver<String>, Guard)>> {
    let request = Request::new(protocol)?;
    let (sender, receiver) = mpsc::sync_channel(QUEUE_LIMIT);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let stop = Arc::new(AtomicBool::new(false));
    let cancelled = stop.clone();
    let worker = std::thread::Builder::new()
        .name("modsync-pipe".into())
        .spawn(move || {
            let result = (|| -> Result<()> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(async {
                    let owner = identity(unsafe { GetCurrentProcess() })?;
                    let name = format!(
                        r"\\.\pipe\modsync-rs-{}-{}-{}",
                        namespace, owner.sid, owner.session
                    );
                    if deliver(&name, &request, &owner).await? {
                        let _ = ready_tx.send(Ok(false));
                        return Ok(());
                    }
                    // FIRST_PIPE_INSTANCE is an atomic ownership claim. A live or
                    // inaccessible endpoint is never torn down or silently replaced.
                    let listener = match server(&name, &owner, true) {
                        Ok(server) => server,
                        Err(error) => {
                            if deliver(&name, &request, &owner).await? {
                                let _ = ready_tx.send(Ok(false));
                                return Ok(());
                            }
                            return Err(error);
                        }
                    };
                    let _ = ready_tx.send(Ok(true));
                    listen(listener, &name, &owner, &sender, &cancelled).await;
                    Ok(())
                })
            })();
            if let Err(error) = result {
                let _ = ready_tx.send(Err(error));
            }
        })?;
    let guard = Guard {
        stop,
        worker: Some(worker),
    };
    match ready_rx
        .recv()
        .context("Named-pipe worker stopped before ownership acknowledgement")??
    {
        true => Ok(Some((receiver, guard))),
        false => {
            drop(guard);
            Ok(None)
        }
    }
}
async fn listen(
    mut listener: NamedPipeServer,
    name: &str,
    owner: &Identity,
    sender: &SyncSender<String>,
    stop: &AtomicBool,
) {
    let mut recent = VecDeque::new();
    while !stop.load(Ordering::Acquire) {
        match timeout(POLL, listener.connect()).await {
            Err(_) => continue,
            Ok(Err(_)) => break,
            Ok(Ok(())) => (),
        }
        // Keep a listening instance alive before dropping the connected one,
        // so another process cannot steal the name during acknowledgement.
        let next = match server(name, owner, false) {
            Ok(next) => next,
            Err(_) => break,
        };
        let _ = timeout(DEADLINE, async {
            peer(listener.as_raw_handle(), true, owner)?;
            let request: Request = serde_json::from_slice(&read_frame(&mut listener).await?)?;
            ensure!(!stop.load(Ordering::Acquire), "Listener stopping");
            let ack = admit(request, sender, &mut recent);
            write_frame(&mut listener, &serde_json::to_vec(&ack)?).await?;
            // Do not close a Windows pipe with its acknowledgement unread.
            // The confirmation proves the client consumed it; no blocking
            // FlushFileBuffers call is needed during listener shutdown.
            ensure!(
                listener.read_u8().await? == 1,
                "Missing acknowledgement confirmation"
            );
            Ok::<(), anyhow::Error>(())
        })
        .await;
        drop(listener);
        listener = next;
    }
}
async fn deliver(name: &str, request: &Request, owner: &Identity) -> Result<bool> {
    let mut last = None;
    for attempt in 0..2 {
        let deadline = Instant::now() + DEADLINE;
        let mut client = loop {
            match ClientOptions::new().open(name) {
                Ok(client) => break client,
                Err(error)
                    if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32)
                        && attempt == 0 =>
                {
                    return Ok(false);
                }
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                    ensure!(
                        Instant::now() < deadline,
                        "Named-pipe connect deadline exceeded"
                    );
                    sleep(POLL).await;
                }
                Err(error) => return Err(error.into()),
            }
        };
        peer(client.as_raw_handle(), false, owner)?;
        let result = timeout(DEADLINE, async {
            write_frame(&mut client, &serde_json::to_vec(request)?).await?;
            let ack = serde_json::from_slice(&read_frame(&mut client).await?)?;
            client.write_u8(1).await?;
            check_ack(request, ack)
        })
        .await;
        match result {
            Ok(Ok(())) => return Ok(true),
            Ok(Err(error)) => last = Some(error),
            Err(error) => last = Some(error.into()),
        }
    }
    Err(last.context("Missing pipe acknowledgement")?)
}
async fn read_frame(stream: &mut (impl AsyncRead + Unpin)) -> Result<Vec<u8>> {
    let size = stream.read_u32().await? as usize;
    ensure!(
        size > 0 && size <= FRAME_LIMIT,
        "Invalid named-pipe frame length"
    );
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(bytes)
}
async fn write_frame(stream: &mut (impl AsyncWrite + Unpin), bytes: &[u8]) -> Result<()> {
    ensure!(bytes.len() <= FRAME_LIMIT, "Named-pipe frame too large");
    stream.write_u32(bytes.len() as u32).await?;
    stream.write_all(bytes).await?;
    Ok(())
}
