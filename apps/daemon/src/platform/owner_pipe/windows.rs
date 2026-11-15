//! Windows: the owner pipe, created through Tokio's named pipes with a security descriptor that
//! makes this user the pipe's owner and the only one who may open it.

use std::ffi::{c_void, OsStr};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::time::{Duration, Instant};

use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::Foundation::{LocalFree, ERROR_PIPE_BUSY, ERROR_SUCCESS};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    EqualSid, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
};
use windows_sys::Win32::Storage::FileSystem::{FlushFileBuffers, SECURITY_IDENTIFICATION};
use windows_sys::Win32::System::Pipes::WaitNamedPipeW;

pub(super) const AVAILABLE: bool = true;

/// A connection [`connect`] made. Only reads and writes: `File`'s own `read_to_end` sizes its
/// buffer from the file's length first, which a pipe does not have, and was seen to end with
/// nothing read where plain reads get the whole answer.
pub struct BlockingStream(File);

impl io::Read for BlockingStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Write for BlockingStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
pub type AsyncStream = NamedPipeClient;

/// How long an asynchronous connect waits between tries while every instance is busy.
const BUSY_PAUSE: Duration = Duration::from_millis(50);

/// How long a connection being shut down waits for the client to read what was written to it.
const DRAIN_WAIT: Duration = Duration::from_secs(10);

/// A connection the listener accepted. Closing a pipe's server end discards what the client has
/// not read yet, so shutting it down first waits, up to [`DRAIN_WAIT`], until the client has read
/// everything written (`FlushFileBuffers`, as Windows' own pipe servers do before they close): an
/// answer written just before the server closes, as one with `Connection: close` is, still arrives.
pub struct Stream {
    pipe: NamedPipeServer,
    draining: Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>,
}

impl tokio::io::AsyncRead for Stream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for Stream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        std::pin::Pin::new(&mut self.pipe).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::pin::Pin::new(&mut self.pipe).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        use std::os::windows::io::{AsRawHandle as _, BorrowedHandle};

        if self.draining.is_none() {
            // SAFETY: the handle is open for as long as `self.pipe` is, which outlives the borrow;
            // the copy made of it is a handle of its own, closed when the wait is over.
            let copy = unsafe { BorrowedHandle::borrow_raw(self.pipe.as_raw_handle()) }
                .try_clone_to_owned();
            self.draining = Some(Box::pin(async move {
                let Ok(copy) = copy else {
                    return;
                };
                // A client that never reads leaves the blocking wait behind until it goes away;
                // the connection is closed at the end of `DRAIN_WAIT` all the same.
                let wait = tokio::task::spawn_blocking(move || {
                    // SAFETY: the copy is open for the call.
                    unsafe { FlushFileBuffers(copy.as_raw_handle()) };
                });
                let _ = tokio::time::timeout(DRAIN_WAIT, wait).await;
            }));
        }
        let draining = self.draining.as_mut().expect("set above");
        std::task::ready!(draining.as_mut().poll(cx));
        std::task::Poll::Ready(Ok(()))
    }
}

fn last_error() -> io::Error {
    io::Error::last_os_error()
}

fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(Some(0)).collect()
}

/// A security descriptor `LocalAlloc` made, freed when dropped.
struct Descriptor(PSECURITY_DESCRIPTOR);

// SAFETY: the descriptor is only read after it is made, and freed once, on drop.
unsafe impl Send for Descriptor {}
// SAFETY: as above.
unsafe impl Sync for Descriptor {}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: made by `ConvertStringSecurityDescriptorToSecurityDescriptorW`, freed only here.
        unsafe { LocalFree(self.0) };
    }
}

/// `sid` in its string form (`S-1-5-21-…`).
fn sid_text(sid: &[u8]) -> io::Result<String> {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: `sid` is a valid SID, alive for the call; the string is freed below.
    if unsafe { ConvertSidToStringSidW(sid.as_ptr() as PSID, &mut text) } == 0 {
        return Err(last_error());
    }
    // SAFETY: the call wrote a NUL-terminated string at `text`.
    let converted = unsafe {
        let length = (0..).take_while(|&i| *text.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(text, length))
    };
    // SAFETY: allocated by the call above, not used after.
    unsafe { LocalFree(text.cast()) };
    Ok(converted)
}

/// The descriptor of the pipe: owned by this user, who alone is granted access, with nothing
/// inherited.
fn owner_only() -> io::Result<Descriptor> {
    let user = sid_text(&crate::platform::fs::process_user_sid()?)?;
    let sddl = wide(&format!("O:{user}D:P(A;;GA;;;{user})"));
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the string is NUL-terminated and alive for the call; the descriptor is owned by the
    // returned value.
    let made = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    };
    if made == 0 {
        return Err(last_error());
    }
    Ok(Descriptor(descriptor))
}

pub(super) struct Listener {
    name: String,
    descriptor: Descriptor,
    /// The instance the next client connects to.
    next: tokio::sync::Mutex<NamedPipeServer>,
}

/// An instance of the pipe `name` with `descriptor`; the first one when `first`, which fails when
/// the name is already held.
fn instance(name: &str, descriptor: &Descriptor, first: bool) -> io::Result<NamedPipeServer> {
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    // SAFETY: the attributes and the descriptor they point to are valid and alive for the call.
    unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .create_with_security_attributes_raw(
                name,
                &mut attributes as *mut SECURITY_ATTRIBUTES as *mut c_void,
            )
    }
}

pub(super) fn bind(name: &str) -> io::Result<Listener> {
    let descriptor = owner_only()?;
    let first = match instance(name, &descriptor, true) {
        Ok(first) => first,
        // The name may still be held by this user's own pipe, of an earlier listener whose
        // connections have not all closed yet (remote access turned off and on again). Joining
        // that pipe is as safe as making a new one, since its owner and its access list are this
        // user's; a pipe anyone else made is refused, whether or not it lets this user in.
        Err(err) => {
            let joined = instance(name, &descriptor, false).map_err(|_| held(&err))?;
            check_owner(joined.as_raw_handle()).map_err(|_| held(&err))?;
            joined
        }
    };
    Ok(Listener {
        name: name.to_owned(),
        descriptor,
        next: tokio::sync::Mutex::new(first),
    })
}

/// The refusal of a name another user's pipe holds, after `err`, the failure to create it.
fn held(err: &io::Error) -> io::Error {
    io::Error::new(
        io::ErrorKind::AddrInUse,
        format!("another program or another user of this computer holds the pipe's name ({err})"),
    )
}

pub(super) async fn accept(listener: &Listener) -> io::Result<Stream> {
    let mut next = listener.next.lock().await;
    let connected = next.connect().await;
    // A fresh instance waits for the next client before this one is handed out; one a client left
    // before it was connected is replaced as well.
    let fresh = instance(&listener.name, &listener.descriptor, false)?;
    let this = std::mem::replace(&mut *next, fresh);
    connected.map(|()| Stream {
        pipe: this,
        draining: None,
    })
}

/// Fails unless the pipe open at `handle` is owned by the user this process runs as.
fn check_owner(handle: std::os::windows::io::RawHandle) -> io::Result<()> {
    let mut owner: PSID = std::ptr::null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: the handle is open with read access, which includes reading its owner; what `owner`
    // points into is the descriptor, freed below after its last use.
    let status = unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(status as i32));
    }
    let same = crate::platform::fs::process_user_sid().map(|user| {
        // SAFETY: both are valid SIDs, alive for the call.
        !owner.is_null() && unsafe { EqualSid(user.as_ptr() as PSID, owner) } != 0
    });
    // SAFETY: allocated by `GetSecurityInfo`; nothing points into it any more.
    unsafe { LocalFree(descriptor) };
    if same? {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the pipe of that name belongs to another user of this computer, not to the daemon's",
        ))
    }
}

fn is_busy(err: &io::Error) -> bool {
    err.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
}

pub(super) fn connect(name: &str, timeout: Duration) -> io::Result<BlockingStream> {
    let deadline = Instant::now() + timeout;
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(name)
        {
            Ok(file) => {
                check_owner(file.as_raw_handle())?;
                return Ok(BlockingStream(file));
            }
            Err(err) if is_busy(&err) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(err);
                }
                let millis = u32::try_from(left.as_millis())
                    .unwrap_or(u32::MAX - 1)
                    .max(1);
                // SAFETY: the name is NUL-terminated and alive for the call. Its answer is not
                // needed: the open is tried again either way.
                unsafe { WaitNamedPipeW(wide(name).as_ptr(), millis) };
            }
            Err(err) => return Err(err),
        }
    }
}

pub(super) async fn connect_async(name: &str, timeout: Duration) -> io::Result<AsyncStream> {
    let deadline = Instant::now() + timeout;
    loop {
        // Tokio's client options ask for the identification level of impersonation alone.
        match ClientOptions::new().open(name) {
            Ok(client) => {
                check_owner(client.as_raw_handle())?;
                return Ok(client);
            }
            Err(err) if is_busy(&err) && Instant::now() < deadline => {
                tokio::time::sleep(BUSY_PAUSE).await;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn unique_name() -> String {
        format!(
            r"\\.\pipe\octoboard-owner-test-{}",
            uuid::Uuid::new_v4().simple()
        )
    }

    /// A client reaches the listener, both ways, and several clients each get an instance.
    #[tokio::test]
    async fn clients_reach_the_listener_one_instance_each() {
        let name = unique_name();
        let listener = bind(&name).unwrap();
        let server = tokio::spawn(async move {
            for _ in 0..2 {
                let mut stream = accept(&listener).await.unwrap();
                tokio::spawn(async move {
                    let mut buffer = [0; 4];
                    stream.read_exact(&mut buffer).await.unwrap();
                    stream.write_all(&buffer).await.unwrap();
                });
            }
        });
        for word in [*b"ping", *b"pong"] {
            let mut client = connect_async(&name, Duration::from_secs(5)).await.unwrap();
            client.write_all(&word).await.unwrap();
            let mut echoed = [0; 4];
            client.read_exact(&mut echoed).await.unwrap();
            assert_eq!(echoed, word);
        }
        server.await.unwrap();
    }

    /// The blocking connect reaches the listener too.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_blocking_connect_reaches_the_listener() {
        use std::io::{Read, Write};

        let name = unique_name();
        let listener = bind(&name).unwrap();
        let server = tokio::spawn(async move {
            let mut stream = accept(&listener).await.unwrap();
            stream.write_all(b"hello").await.unwrap();
        });
        let mut client =
            tokio::task::spawn_blocking(move || connect(&name, Duration::from_secs(5)).unwrap())
                .await
                .unwrap();
        server.await.unwrap();
        let mut greeting = [0; 5];
        client.read_exact(&mut greeting).unwrap();
        assert_eq!(&greeting, b"hello");
        let _ = client.write(b"x");
    }

    /// A name this user's own pipe still holds (an earlier listener's connections not yet
    /// closed) is joined rather than refused, and a client reaches the new listener.
    #[tokio::test]
    async fn a_name_this_users_own_pipe_still_holds_is_joined() {
        let name = unique_name();
        let earlier = bind(&name).unwrap();
        let lingering = connect_async(&name, Duration::from_secs(5)).await.unwrap();
        drop(earlier);
        let listener = bind(&name).expect("this user's own pipe is joined");
        let server = tokio::spawn(async move {
            let mut stream = accept(&listener).await.unwrap();
            stream.write_all(b"new").await.unwrap();
        });
        let mut client = connect_async(&name, Duration::from_secs(5)).await.unwrap();
        let mut greeting = [0; 3];
        client.read_exact(&mut greeting).await.unwrap();
        assert_eq!(&greeting, b"new");
        server.await.unwrap();
        drop(lingering);
    }

    /// The listener's pipe names this user as its owner, even in an elevated process, whose
    /// default owner is the Administrators group: a client's check passes.
    #[tokio::test]
    async fn the_listeners_pipe_is_owned_by_this_user() {
        let name = unique_name();
        let _listener = bind(&name).unwrap();
        let client = ClientOptions::new().open(&name).unwrap();
        check_owner(client.as_raw_handle()).expect("owned by this user");
    }

    /// Nothing listening is an error, not a wait.
    #[test]
    fn nothing_listening_is_an_error() {
        assert!(connect(&unique_name(), Duration::from_millis(100)).is_err());
    }
}
