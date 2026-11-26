//! Launch isolated worker processes with an OS-enforced memory limit applied before untrusted input.
//!
//! Windows uses a Job Object in the worker and Linux uses RLIMIT_AS. macOS's
//! RLIMIT_AS is an alias of RLIMIT_RSS and rejects the required memory limits.
//! On macOS the kernel-enforced alternative is a fatal Jetsam *spawn* limit:
//! the parent must apply it with posix_spawnattr_setjetsam_ext before exec.

use std::io::{self, Read, Write};
use std::path::Path;
use std::process::ExitStatus;

pub(crate) const WORKER_MEMORY_LIMIT_BYTES: usize = 512 * 1024 * 1024;
pub(crate) const PLUGIN_WORKER_MEMORY_LIMIT_BYTES: usize = 128 * 1024 * 1024;
pub(crate) const OPERATIONS_BOOTSTRAP_PROOF: &[u8] = b"opendash-operations-jetsam-512mib-v1";
pub(crate) const PLUGIN_BOOTSTRAP_PROOF: &[u8] = b"opendash-plugin-jetsam-128mib-v1";

pub(crate) struct WorkerSpawnConfig<'a> {
    pub(crate) exe: &'a Path,
    pub(crate) args: &'a [&'a str],
    pub(crate) env_vars: &'a [(&'a str, &'a str)],
    pub(crate) memory_limit_bytes: usize,
    pub(crate) macos_bootstrap_proof: &'a [u8],
    pub(crate) inherit_stderr: bool,
}

pub(crate) enum GuardedChild {
    #[cfg(not(target_os = "macos"))]
    Std(std::process::Child),
    #[cfg(target_os = "macos")]
    Mac(OperationsChild),
}

impl GuardedChild {
    pub(crate) fn take_stdin(&mut self) -> Option<Box<dyn Write + Send>> {
        match self {
            #[cfg(not(target_os = "macos"))]
            Self::Std(child) => child
                .stdin
                .take()
                .map(|s| Box::new(s) as Box<dyn Write + Send>),
            #[cfg(target_os = "macos")]
            Self::Mac(child) => child
                .stdin
                .take()
                .map(|s| Box::new(s) as Box<dyn Write + Send>),
        }
    }

    pub(crate) fn take_stdout(&mut self) -> Option<Box<dyn Read + Send>> {
        match self {
            #[cfg(not(target_os = "macos"))]
            Self::Std(child) => child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
            #[cfg(target_os = "macos")]
            Self::Mac(child) => child
                .take_stdout()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        }
    }

    pub(crate) fn kill(&mut self) -> io::Result<()> {
        match self {
            #[cfg(not(target_os = "macos"))]
            Self::Std(child) => child.kill(),
            #[cfg(target_os = "macos")]
            Self::Mac(child) => child.kill(),
        }
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        match self {
            #[cfg(not(target_os = "macos"))]
            Self::Std(child) => child.wait(),
            #[cfg(target_os = "macos")]
            Self::Mac(child) => child.wait(),
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn spawn_worker_process(cfg: WorkerSpawnConfig<'_>) -> io::Result<GuardedChild> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(cfg.exe);
    cmd.args(cfg.args);
    for (k, v) in cfg.env_vars {
        cmd.env(k, v);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    if cfg.inherit_stderr {
        cmd.stderr(Stdio::inherit());
    } else {
        cmd.stderr(Stdio::null());
    }
    cmd.spawn().map(GuardedChild::Std)
}

#[cfg(target_os = "macos")]
pub(crate) fn spawn_worker_process(cfg: WorkerSpawnConfig<'_>) -> io::Result<GuardedChild> {
    macos::spawn_with_config(cfg).map(GuardedChild::Mac)
}

#[cfg(windows)]
pub(crate) fn enforce_native_memory_limit_bytes(
    limit_bytes: usize,
    _macos_proof: &[u8],
) -> Result<(), String> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let job = CreateJobObjectW(null_mut(), null_mut());
        if job.is_null() || job == INVALID_HANDLE_VALUE {
            return Err("CreateJobObjectW failed".to_string());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.ProcessMemoryLimit = limit_bytes;

        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        if ok == 0 {
            CloseHandle(job);
            return Err("SetInformationJobObject failed".to_string());
        }
        let assigned = AssignProcessToJobObject(job, GetCurrentProcess());
        if assigned == 0 {
            CloseHandle(job);
            return Err("AssignProcessToJobObject failed".to_string());
        }
        // Keep job handle open for process lifetime so KILL_ON_JOB_CLOSE stays bound.
        let _ = job;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) fn enforce_native_memory_limit_bytes(
    limit_bytes: usize,
    _macos_proof: &[u8],
) -> Result<(), String> {
    #[repr(C)]
    struct RLimit {
        rlim_cur: u64,
        rlim_max: u64,
    }
    const RLIMIT_AS: i32 = 9;
    unsafe extern "C" {
        fn setrlimit(resource: i32, rlim: *const RLimit) -> i32;
    }
    let lim = RLimit {
        rlim_cur: limit_bytes as u64,
        rlim_max: limit_bytes as u64,
    };
    let rc = unsafe { setrlimit(RLIMIT_AS, &lim) };
    if rc != 0 {
        return Err(format!("setrlimit(RLIMIT_AS) failed with {rc}"));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn enforce_native_memory_limit_bytes(
    _limit_bytes: usize,
    macos_proof: &[u8],
) -> Result<(), String> {
    // Darwin rejects RLIMIT_AS (EINVAL); the parent must apply the kernel
    // fatal Jetsam limit with posix_spawnattr_setjetsam_ext before exec.
    macos::confirm_parent_spawn_limit_with_proof(macos_proof)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub(crate) fn enforce_native_memory_limit_bytes(
    _limit_bytes: usize,
    _macos_proof: &[u8],
) -> Result<(), String> {
    Err("Không hỗ trợ giới hạn bộ nhớ native cho nền tảng này.".to_string())
}

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
    use std::collections::BTreeMap;
    use std::ffi::{CString, OsString};
    use std::io::{self, Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::os::unix::net::UnixStream;
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::process::{ExitStatus, Output};

    use super::{OPERATIONS_BOOTSTRAP_PROOF, WORKER_MEMORY_LIMIT_BYTES, WorkerSpawnConfig};

    // These flags are defined by Apple's spawn_private.h (fatal in both states).
    const JETSAM_ACTIVE_FATAL: libc::c_short = 0x04;
    const JETSAM_INACTIVE_FATAL: libc::c_short = 0x08;
    // FD 3 is a private bootstrap channel, not a caller-provided environment
    // variable. Directly executing a worker must fail closed.
    pub(crate) const BOOTSTRAP_FD: libc::c_int = 3;

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
        pub(crate) fn take_stdout(&mut self) -> Option<UnixStream> {
            self.stdout.take()
        }

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
        spawn_with_config(WorkerSpawnConfig {
            exe,
            args,
            env_vars: &[],
            memory_limit_bytes: WORKER_MEMORY_LIMIT_BYTES,
            macos_bootstrap_proof: OPERATIONS_BOOTSTRAP_PROOF,
            inherit_stderr: false,
        })
    }

    pub(crate) fn spawn_with_config(cfg: WorkerSpawnConfig<'_>) -> io::Result<OperationsChild> {
        let jetsam_mib = (cfg.memory_limit_bytes / (1024 * 1024)).max(1) as libc::c_int;
        let exe_bytes = CString::new(cfg.exe.as_os_str().as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "executable path contains NUL")
        })?;
        let args = cfg
            .args
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
        let mut env_map: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
        for (k, v) in cfg.env_vars {
            env_map.insert(OsString::from(k), OsString::from(v));
        }
        let environment = env_map
            .into_iter()
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
                    jetsam_mib,
                    jetsam_mib,
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
            if !cfg.inherit_stderr {
                returned_error(unsafe {
                    libc::posix_spawn_file_actions_addopen(
                        &mut actions,
                        libc::STDERR_FILENO,
                        c"/dev/null".as_ptr(),
                        libc::O_WRONLY,
                        0,
                    )
                })?;
            }

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
        if let Err(err) = proof_parent.write_all(cfg.macos_bootstrap_proof) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
        Ok(child)
    }

    pub(crate) fn confirm_parent_spawn_limit() -> Result<(), String> {
        confirm_parent_spawn_limit_with_proof(OPERATIONS_BOOTSTRAP_PROOF)
    }

    pub(crate) fn confirm_parent_spawn_limit_with_proof(
        expected_proof: &[u8],
    ) -> Result<(), String> {
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
        let mut actual = vec![0u8; expected_proof.len()];
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
        if actual != expected_proof {
            return Err("invalid macOS Jetsam spawn proof".to_string());
        }
        unsafe { libc::close(BOOTSTRAP_FD) };
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub(crate) use macos::OperationsChild;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub(crate) use macos::confirm_parent_spawn_limit;
#[cfg(target_os = "macos")]
pub(crate) use macos::spawn;
#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub(crate) use macos::spawn_with_args;
