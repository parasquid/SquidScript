mod command;
mod lifecycle;
mod line;
mod log;
mod runtime;
mod state;
mod vm;

pub use command::handle_command;
pub use lifecycle::{boot_main, storage_error_from_persistent};
pub use line::{trim_ascii, LineBuffer};
pub use runtime::RuntimeSink;
pub use vm::{ActiveVm, TempApp};

pub const BUILD_ID: &str = match option_env!("SQUID_FIRMWARE_BUILD_ID") {
    Some(value) => value,
    None => "dev-build",
};

pub(super) const INSTALL_TIMEOUT_MS: u32 = 2_000;
pub(super) const MEMORY_AVAILABLE_BYTES: usize = 311_416;
