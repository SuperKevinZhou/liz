//! CLI workspace skeleton for liz.

pub mod app_client;
pub mod renderers;
pub mod view_model;

/// Returns a short banner that is useful for smoke tests and manual sanity checks.
pub fn banner_line() -> String {
    format!(
        "{} [{} | {} | {}]",
        "liz-cli workspace skeleton",
        app_client::AppClientSkeleton::default().transport,
        view_model::ViewModelSkeleton::default().primary_view,
        renderers::RendererSkeleton::default().renderer_stack,
    )
}
