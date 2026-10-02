use super::*;
use anyhow::Context;
use fs2::FileExt;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    sync::mpsc,
    time::Instant,
};

pub(super) fn claim_or_forward(
    protocol: Option<&str>,
    namespace: &str,
) -> Result<Option<(Receiver<String>, Guard)>> {
    let uid = unsafe { libc::geteuid() };
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = base.join(format!("modsync-rs-{namespace}-{uid}"));
    match fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error.into()),
    }
    private_directory(&directory)?;
    claim_at(directory.join("handoff.sock"), protocol)
}

fn private_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0,
        "Single-instance directory must be owned by this user and private"
    );
    Ok(())
}

struct Owner {
    path: PathBuf,
    device: u64,
    inode: u64,
    _lock: File,
}
impl Drop for Owner {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| {
            m.file_type().is_socket() && m.dev() == self.device && m.ino() == self.inode
        }) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

pub(super) fn claim_at(
    path: PathBuf,
    protocol: Option<&str>,
) -> Result<Option<(Receiver<String>, Guard)>> {
    private_directory(path.parent().context("Socket needs parent")?)?;
    let request = Request::new(protocol)?;
    // Only a connect-level missing/refused result permits ownership recovery.
    if deliver(&path, &request)? {
        return Ok(None);
    }
    let lock_path = path.with_extension("lock");
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(&lock_path)?;
    let metadata = lock.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::geteuid() }
            && metadata.mode() & 0o077 == 0
            && metadata.nlink() == 1,
        "Unsafe single-instance ownership lock"
    );
    let deadline = Instant::now() + DEADLINE;
    loop {
        match lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if deliver(&path, &request)? {
                    return Ok(None);
                }
                ensure!(
                    Instant::now() < deadline,
                    "Primary owns its lock but has no responsive endpoint"
                );
                std::thread::sleep(POLL);
            }
            Err(error) => return Err(error.into()),
        }
    }
    // Second owner probe while serialized. Never unlink on timeout/access/write failure.
    if deliver(&path, &request)? {
        return Ok(None);
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            ensure!(
                metadata.file_type().is_socket() && metadata.uid() == unsafe { libc::geteuid() },
                "Refusing to replace foreign or non-socket endpoint"
            );
            fs::remove_file(&path)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let listener = UnixListener::bind(&path)?;
    let metadata = fs::symlink_metadata(&path)?;
    let owner = Owner {
        path: path.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
        _lock: lock,
    };
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    let (sender, receiver) = mpsc::sync_channel(QUEUE_LIMIT);
    let stop = Arc::new(AtomicBool::new(false));
    let cancelled = stop.clone();
    let worker = std::thread::Builder::new()
        .name("modsync-handoff".into())
        .spawn(move || {
            let _owner = owner;
            let mut recent = VecDeque::new();
            while !cancelled.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = (|| -> Result<()> {
                            peer(&stream)?;
                            stream.set_nonblocking(true)?;
                            let deadline = Instant::now() + DEADLINE;
                            let request: Request = serde_json::from_slice(&read_frame(
                                &mut stream,
                                deadline,
                                &cancelled,
                            )?)?;
                            let ack = admit(request, &sender, &mut recent);
                            write_frame(
                                &mut stream,
                                &serde_json::to_vec(&ack)?,
                                deadline,
                                &cancelled,
                            )
                        })();
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(POLL)
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                    Err(_) => break,
                }
            }
        })?;
    Ok(Some((
        receiver,
        Guard {
            stop,
            worker: Some(worker),
        },
    )))
}

fn deliver(path: &Path, request: &Request) -> Result<bool> {
    // Retry once with the same request ID after an uncertain acknowledgement.
    let mut last = None;
    for attempt in 0..2 {
        let mut stream = match connect(path) {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) && attempt == 0 =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error.into()),
        };
        peer(&stream)?;
        stream.set_nonblocking(true)?;
        let flag = AtomicBool::new(false);
        let deadline = Instant::now() + DEADLINE;
        let result = (|| -> Result<()> {
            write_frame(&mut stream, &serde_json::to_vec(request)?, deadline, &flag)?;
            check_ack(
                request,
                serde_json::from_slice(&read_frame(&mut stream, deadline, &flag)?)?,
            )
        })();
        match result {
            Ok(()) => return Ok(true),
            Err(error) => last = Some(error),
        }
    }
    Err(last.context("Missing handoff result")?)
}

fn connect(path: &Path) -> io::Result<UnixStream> {
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Socket path is too long or contains NUL",
        ));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        address.sun_len = std::mem::size_of_val(&address) as u8;
    }
    for (target, source) in address.sun_path.iter_mut().zip(bytes) {
        *target = *source as libc::c_char;
    }
    let fd = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    stream.set_nonblocking(true)?;
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let deadline = Instant::now() + DEADLINE;
    loop {
        if unsafe {
            libc::connect(
                fd,
                (&address as *const libc::sockaddr_un).cast(),
                std::mem::size_of_val(&address) as libc::socklen_t,
            )
        } == 0
        {
            return Ok(stream);
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EISCONN) {
            return Ok(stream);
        }
        if !matches!(
            error.raw_os_error(),
            Some(libc::EINPROGRESS | libc::EALREADY | libc::EAGAIN | libc::EINTR)
        ) {
            return Err(error);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Socket connect deadline exceeded",
            ));
        }
        std::thread::sleep(POLL);
    }
}

fn check_time(deadline: Instant, stop: &AtomicBool) -> Result<()> {
    ensure!(
        !stop.load(Ordering::Acquire),
        "Single-instance listener stopped"
    );
    ensure!(
        Instant::now() < deadline,
        "Single-instance I/O deadline exceeded"
    );
    Ok(())
}
fn read_exact(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<()> {
    while !bytes.is_empty() {
        check_time(deadline, stop)?;
        match stream.read(bytes) {
            Ok(0) => anyhow::bail!("Truncated handoff frame"),
            Ok(n) => bytes = &mut bytes[n..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(POLL),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn read_frame(stream: &mut UnixStream, deadline: Instant, stop: &AtomicBool) -> Result<Vec<u8>> {
    let mut size = [0; 4];
    read_exact(stream, &mut size, deadline, stop)?;
    let size = u32::from_be_bytes(size) as usize;
    ensure!(
        size > 0 && size <= FRAME_LIMIT,
        "Invalid handoff frame size"
    );
    let mut bytes = vec![0; size];
    read_exact(stream, &mut bytes, deadline, stop)?;
    Ok(bytes)
}
fn write_frame(
    stream: &mut UnixStream,
    bytes: &[u8],
    deadline: Instant,
    stop: &AtomicBool,
) -> Result<()> {
    ensure!(bytes.len() <= FRAME_LIMIT, "Handoff frame exceeds limit");
    let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
    frame.extend_from_slice(bytes);
    let mut rest = frame.as_slice();
    while !rest.is_empty() {
        check_time(deadline, stop)?;
        match stream.write(rest) {
            Ok(0) => anyhow::bail!("Handoff write stopped"),
            Ok(n) => rest = &rest[n..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(POLL),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn peer(stream: &UnixStream) -> Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let uid = {
        let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of_val(&credentials) as libc::socklen_t;
        ensure!(
            unsafe {
                libc::getsockopt(
                    stream.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_PEERCRED,
                    (&mut credentials as *mut libc::ucred).cast(),
                    &mut size,
                )
            } == 0,
            "Cannot authenticate socket peer"
        );
        credentials.uid
    };
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    let uid = {
        let mut uid = 0;
        let mut gid = 0;
        ensure!(
            unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == 0,
            "Cannot authenticate socket peer"
        );
        uid
    };
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )))]
    {
        let _ = stream;
        anyhow::bail!("Peer authentication unsupported on this Unix target");
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    ensure!(
        uid == unsafe { libc::geteuid() },
        "Socket peer belongs to another user"
    );
    Ok(())
}
