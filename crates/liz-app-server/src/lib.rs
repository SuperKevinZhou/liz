//! App-server workspace skeleton for the liz runtime.

pub mod events;
pub mod handlers;
pub mod runtime;
pub mod server;
pub mod storage;

/// Returns a short banner that is useful for smoke tests and manual sanity checks.
pub fn banner_line() -> String {
    format!(
        "{} [{} | {} | {} | {}]",
        "liz-app-server workspace skeleton",
        server::ServerConfig::default().bind_address,
        runtime::RuntimeSkeleton::default().mode,
        storage::StorageLayout::default().root_dir,
        events::EventStreamSkeleton::default().transport,
    )
}
