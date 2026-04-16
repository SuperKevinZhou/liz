//! Smoke tests for the liz app-server binary entrypoint.

use std::process::Command;

/// Verifies that the app-server binary starts and prints the expected bootstrap banner.
#[test]
fn app_server_binary_prints_workspace_banner() {
    let output = Command::new(env!("CARGO_BIN_EXE_liz-app-server"))
        .output()
        .expect("app-server binary should be executable in smoke tests");

    assert!(
        output.status.success(),
        "app-server binary should exit successfully: {:?}",
        output.status
    );

    let stdout =
        String::from_utf8(output.stdout).expect("app-server smoke output should be valid UTF-8");

    assert!(stdout.contains("liz-app-server runtime"), "unexpected app-server banner: {stdout}");
    assert!(
        stdout.contains("websocket"),
        "app-server banner should surface websocket wiring: {stdout}"
    );
}
