//! CLI workspace for liz.

pub mod app_client;
pub mod renderers;
pub mod tui;
pub mod view_model;

/// Returns a short banner that is useful for smoke tests and manual sanity checks.
pub fn banner_line() -> String {
    format!(
        "{} [{} | {} | {}]",
        "liz-cli chat shell",
        app_client::WebSocketAppClient::transport_name(),
        view_model::ViewModel::primary_view(),
        renderers::RendererSkeleton::default().renderer_stack,
    )
}
