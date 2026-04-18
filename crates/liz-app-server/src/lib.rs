//! App-server workspace skeleton for the liz runtime.

pub mod config;
pub mod events;
pub mod executor;
pub mod handlers;
pub mod model;
pub mod runtime;
pub mod server;
pub mod storage;

/// Returns a short banner that is useful for smoke tests and manual sanity checks.
pub fn banner_line() -> String {
    format!(
        "{} [{} | {} | {} | {}]",
        "liz-app-server runtime",
        server::ServerConfig::default().bind_address,
        runtime::RuntimeCoordinator::default_mode(),
        storage::StorageLayout::default().root_dir,
        events::EventStreamSkeleton::default().transport,
    )
}
