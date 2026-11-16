//! Launch operations workers with a memory limit applied before untrusted input.
//!
//! Windows uses a Job Object in the worker and Linux uses RLIMIT_AS. macOS's
//! RLIMIT_AS is an alias of RLIMIT_RSS and rejects the required 512 MiB limit.
//! On macOS the kernel-enforced alternative is a fatal Jetsam *spawn* limit:
//! the parent must apply it with posix_spawnattr_setjetsam_ext before exec.

#[cfg(not(target_os = "macos"))]
use std::{io, path::Path};

pub(crate) const WORKER_MEMORY_LIMIT_BYTES: usize = 512 * 1024 * 1024;

#[cfg(not(target_os = "macos"))]
pub(crate) type OperationsChild = std::process::Child;

#[cfg(not(target_os = "macos"))]
pub(crate) fn spawn(exe: &Path, mode: &str) -> io::Result<OperationsChild> {
    use std::process::{Command, Stdio};
    Command::new(exe)
        .arg("--operations-worker")
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::CString;
    use std::io::{self, Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::process::{ExitStatus, Output};

    use super::WORKER_MEMORY_LIMIT_BYTES;

    const JETSAM_MEMORY_MIB: libc::c_int = (WORKER_MEMORY_LIMIT_BYTES / (1024 * 1024)) as _;
    // These flags are defined by Apple's spawn_private.h (fatal in both states).
    const JETSAM_ACTIVE_FATAL: libc::c_short = 0x04;
    const JETSAM_INACTIVE_FATAL: libc::c_short = 0x08;
    // FD 3 is a private bootstrap channel, not a caller-provided environment
    // variable. Directly executing --operations-worker must fail closed.
    pub(crate) const BOOTSTRAP_FD: libc::c_int = 3;
    pub(crate) const BOOTSTRAP_PROOF: &[u8] = b"opendash-operations-jetsam-512mib-v1";

    // posix_spawnattr_setjetsam_ext is a private, OS-version-specific API.
    // Resolve dynamically and refuse to spawn if macOS does not provide it.
    type SetJetsam = unsafe extern "C" fn(
        *mut libc::posix_spawnattr_t,
        libc::c_short,
        libc::c_int,
        libc::c_int,
        libc::c_int,
    ) -> libc::c_int;

    pub(crate) struct OperationsChild {
        pub(crate) stdin: Option<UnixStream>,
        stdout: Option<UnixStream>,
        pid: libc::pid_t,
        exited: Option<ExitStatus>,
    }

    impl OperationsChild {
        pub(crate) fn kill(&mut self) -> io::Result<()> {
            if self.exited.is_some() {
                return Ok(());
            }
            if unsafe { libc::kill(self.pid, libc::SIGKILL) } == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }

        pub(crate) fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            self.reap(libc::WNOHANG)
        }

        pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
            loop {
                if let Some(status) = self.reap(0)? {
                    return Ok(status);
                }
            }
        }

        fn reap(&mut self, options: libc::c_int) -> io::Result<Option<ExitStatus>> {
            if let Some(status) = self.exited {
                return Ok(Some(status));
            }
            let mut raw_status = 0;
            let rc = unsafe { libc::waitpid(self.pid, &mut raw_status, options) };
            if rc == 0 {
                return Ok(None);
            }
            if rc == self.pid {
                let status = ExitStatus::from_raw(raw_status);
                self.exited = Some(status);
                return Ok(Some(status));
            }
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                return Ok(None);
            }
            Err(err)
        }

        pub(crate) fn wait_with_output(mut self) -> io::Result<Output> {
            // Drain the response pipe before blocking on waitpid, like the
            // standard Child::wait_with_output, to avoid a full-pipe deadlock.
            let mut stdout = Vec::new();
            if let Some(mut pipe) = self.stdout.take() {
                pipe.read_to_end(&mut stdout)?;
            }
            let status = self.wait()?;
            Ok(Output {
                status,
                stdout,
                stderr: Vec::new(),
            })
        }
    }

    fn returned_error(rc: libc::c_int) -> io::Result<()> {
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(rc))
        }
    }

    pub(crate) fn spawn(exe: &Path, mode: &str) -> io::Result<OperationsChild> {
        spawn_with_args(exe, &["--operations-worker", mode])
    }

    pub(crate) fn spawn_with_args(exe: &Path, args: &[&str]) -> io::Result<OperationsChild> {
        let exe_bytes = CString::new(exe.as_os_str().as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "executable path contains NUL")
        })?;
        let args = args
            .iter()
            .map(|arg| {
                CString::new(*arg).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "worker arg contains NUL")
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let mut argv = Vec::with_capacity(args.len() + 2);
        argv.push(exe_bytes.as_ptr());
        argv.extend(args.iter().map(|arg| arg.as_ptr()));
        argv.push(std::ptr::null());

        // Snapshot the environment through Rust instead of passing the
        // process-global mutable environ pointer across a concurrent spawn.
        let environment = std::env::vars_os()
            .map(|(key, value)| {
                let mut pair = key.into_vec();
                pair.push(b'=');
                pair.extend(value.into_vec());
                CString::new(pair).map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "environment contains NUL")
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let mut envp = environment
            .iter()
            .map(|item| item.as_ptr())
            .collect::<Vec<_>>();
        envp.push(std::ptr::null());

        // UnixStream::pair() creates close-on-exec descriptors; only the
        // explicit dup2 targets (stdin/stdout/bootstrap) reach the child.
        let (stdin_parent, stdin_child) = UnixStream::pair()?;
        let (stdout_parent, stdout_child) = UnixStream::pair()?;
        let (mut proof_parent, proof_child) = UnixStream::pair()?;

        let mut attributes = unsafe { std::mem::zeroed::<libc::posix_spawnattr_t>() };
        returned_error(unsafe { libc::posix_spawnattr_init(&mut attributes) })?;
        let mut actions = unsafe { std::mem::zeroed::<libc::posix_spawn_file_actions_t>() };
        let actions_init =
            returned_error(unsafe { libc::posix_spawn_file_actions_init(&mut actions) });
        if let Err(err) = actions_init {
            unsafe { libc::posix_spawnattr_destroy(&mut attributes) };
            return Err(err);
        }

        let result = (|| -> io::Result<libc::pid_t> {
            let symbol = unsafe {
                libc::dlsym(
                    libc::RTLD_DEFAULT,
                    c"posix_spawnattr_setjetsam_ext".as_ptr(),
                )
            };
            if symbol.is_null() {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "macOS fatal Jetsam spawn limit unavailable",
                ));
            }
            let set_jetsam: SetJetsam = unsafe { std::mem::transmute(symbol) };
            returned_error(unsafe {
                set_jetsam(
                    &mut attributes,
                    JETSAM_ACTIVE_FATAL | JETSAM_INACTIVE_FATAL,
                    0,
                    JETSAM_MEMORY_MIB,
                    JETSAM_MEMORY_MIB,
                )
            })?;

            returned_error(unsafe {
                libc::posix_spawn_file_actions_adddup2(
                    &mut actions,
                    stdin_child.as_raw_fd(),
                    libc::STDIN_FILENO,
                )
            })?;
            returned_error(unsafe {
                libc::posix_spawn_file_actions_adddup2(
                    &mut actions,
                    stdout_child.as_raw_fd(),
                    libc::STDOUT_FILENO,
                )
            })?;
            returned_error(unsafe {
                libc::posix_spawn_file_actions_adddup2(
                    &mut actions,
                    proof_child.as_raw_fd(),
                    BOOTSTRAP_FD,
                )
            })?;
            returned_error(unsafe {
                libc::posix_spawn_file_actions_addopen(
                    &mut actions,
                    libc::STDERR_FILENO,
                    c"/dev/null".as_ptr(),
                    libc::O_WRONLY,
                    0,
                )
            })?;

            let mut pid: libc::pid_t = 0;
            let rc = unsafe {
                libc::posix_spawn(
                    &mut pid,
                    exe_bytes.as_ptr(),
                    &actions,
                    &attributes,
                    argv.as_ptr() as *const *mut libc::c_char,
                    envp.as_ptr() as *const *mut libc::c_char,
                )
            };
            returned_error(rc)?;
            Ok(pid)
        })();
        unsafe {
            libc::posix_spawn_file_actions_destroy(&mut actions);
            libc::posix_spawnattr_destroy(&mut attributes);
        }
        let pid = result?;
        drop(stdin_child);
        drop(stdout_child);
        drop(proof_child);
        let mut child = OperationsChild {
            stdin: Some(stdin_parent),
            stdout: Some(stdout_parent),
            pid,
            exited: None,
        };
        if let Err(err) = proof_parent.write_all(BOOTSTRAP_PROOF) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
        Ok(child)
    }

    pub(crate) fn confirm_parent_spawn_limit() -> Result<(), String> {
        // The guarded parent supplies this bootstrap channel, rather than
        // inheriting a forgeable environment variable. It is NOT a kernel
        // attestation: the spawn attribute must still be stress-verified.
        // A directly invoked CLI worker may have a different descriptor 3
        // owned by the Rust runtime. Do not adopt it with from_raw_fd(): an
        // OwnedFd can double-close a descriptor it does not own.
        let mut socket_type: libc::c_int = 0;
        let mut size = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        if unsafe {
            libc::getsockopt(
                BOOTSTRAP_FD,
                libc::SOL_SOCKET,
                libc::SO_TYPE,
                (&mut socket_type as *mut libc::c_int).cast(),
                &mut size,
            )
        } != 0
            || socket_type != libc::SOCK_STREAM
        {
            return Err("missing macOS Jetsam spawn proof".to_string());
        }
        let mut pollfd = libc::pollfd {
            fd: BOOTSTRAP_FD,
            events: libc::POLLIN,
            revents: 0,
        };
        if unsafe { libc::poll(&mut pollfd, 1, 2000) } <= 0 {
            return Err("macOS Jetsam spawn proof timed out".to_string());
        }
        let mut actual = vec![0u8; BOOTSTRAP_PROOF.len()];
        let mut received = 0;
        while received < actual.len() {
            let size = unsafe {
                libc::recv(
                    BOOTSTRAP_FD,
                    actual[received..].as_mut_ptr().cast(),
                    actual.len() - received,
                    0,
                )
            };
            if size <= 0 {
                return Err("missing macOS Jetsam spawn proof".to_string());
            }
            received += size as usize;
        }
        if actual != BOOTSTRAP_PROOF {
            return Err("invalid macOS Jetsam spawn proof".to_string());
        }
        unsafe { libc::close(BOOTSTRAP_FD) };
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(crate) use macos::confirm_parent_spawn_limit;
#[cfg(target_os = "macos")]
pub(crate) use macos::spawn;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub(crate) use macos::spawn_with_args;
