use super::*;

pub(crate) struct WasmStoreData {
    pub(crate) host: HostContext,
    pub(crate) limits: StoreLimits,
}

pub const INVOCATION_FUEL: u64 = 20_000_000;

pub const MAX_LINEAR_MEMORY_BYTES: usize = 16 * 1024 * 1024; // 16 MB

pub const INVOCATION_DEADLINE: Duration = Duration::from_secs(5);

pub const MAX_CONCURRENT_INVOCATIONS: usize = 4;

pub const MAX_BACKGROUND_PUBLIC_INVOCATIONS: usize = 2;

#[allow(dead_code)]
pub const MAX_TOTAL_CONCURRENT_WORKERS: usize =
    MAX_CONCURRENT_INVOCATIONS + MAX_BACKGROUND_PUBLIC_INVOCATIONS;

#[derive(Debug)]
pub enum PluginError {
    Disabled,
    NotFound,
    ModuleCompilation(String),
    OutOfFuel,
    Timeout,
    BadPointer,
    GuestFailed(i32),
    Trap(String),
    InvalidOutput(String),
    ConcurrencyLimit,
    Internal(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "Plugin chưa được bật hoặc đã bị vô hiệu hóa."),
            Self::NotFound => write!(f, "Plugin không tồn tại."),
            Self::ModuleCompilation(msg) => write!(f, "Module WebAssembly không hợp lệ: {msg}"),
            Self::OutOfFuel => write!(
                f,
                "Plugin hết fuel (vượt quá giới hạn CPU {INVOCATION_FUEL} đơn vị)."
            ),
            Self::Timeout => write!(f, "Quá thời gian thực thi (deadline 5 giây)."),
            Self::BadPointer => {
                write!(
                    f,
                    "Lỗi con trỏ bộ nhớ (Bad Pointer hoặc ngoài biên memory)."
                )
            }
            Self::GuestFailed(code) => write!(f, "Plugin kết thúc với mã lỗi {code}."),
            Self::Trap(msg) => write!(f, "Guest trap: {msg}"),
            Self::InvalidOutput(msg) => write!(f, "Dữ liệu đầu ra từ plugin không hợp lệ: {msg}"),
            Self::ConcurrencyLimit => write!(f, "Không thể lấy permit điều phối plugin."),
            Self::Internal(msg) => write!(f, "Lỗi máy chủ nội bộ: {msg}"),
        }
    }
}

impl std::error::Error for PluginError {}

pub const MAX_PUBLIC_RENDER_INVOCATIONS: usize = 4;

#[derive(Clone)]
pub struct PluginRuntime {
    pub(crate) engine: Engine,
    pub(crate) admin_semaphore: Arc<Semaphore>,
    pub(crate) background_public_semaphore: Arc<Semaphore>,
    pub(crate) public_render_semaphore: Arc<Semaphore>,
    pub(crate) active_workers: Arc<AtomicUsize>,
    pub(crate) secrets_root: PathBuf,
}

pub(crate) fn scan_ipc_magic<R: std::io::Read>(reader: &mut R) -> Result<(), PluginError> {
    let mut matched = 0usize;
    let mut byte = [0u8; 1];
    for _ in 0..8192 {
        if reader.read_exact(&mut byte).is_err() {
            return Err(PluginError::Trap(
                "Worker process kết thúc trước khi gửi IPC magic".into(),
            ));
        }
        if byte[0] == IPC_STREAM_MAGIC[matched] {
            matched += 1;
            if matched == IPC_STREAM_MAGIC.len() {
                return Ok(());
            }
        } else {
            matched = usize::from(byte[0] == IPC_STREAM_MAGIC[0]);
        }
    }
    Err(PluginError::Trap(
        "Không tìm thấy IPC stream magic hợp lệ từ worker".into(),
    ))
}
