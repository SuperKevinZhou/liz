//! Smoke tests for the liz CLI binary entrypoint.

use std::process::Command;

/// Verifies that the CLI binary starts and prints the expected bootstrap banner.
#[test]
fn cli_binary_prints_workspace_banner() {
    let output = Command::new(env!("CARGO_BIN_EXE_liz-cli"))
        .arg("--banner")
        .output()
        .expect("cli binary should be executable in smoke tests");

    assert!(output.status.success(), "cli binary should exit successfully: {:?}", output.status);

    let stdout = String::from_utf8(output.stdout).expect("cli smoke output should be valid UTF-8");

    assert!(stdout.contains("liz-cli chat shell"), "unexpected cli banner: {stdout}");
    assert!(
        stdout.contains("transcript"),
        "cli banner should surface transcript rendering intent: {stdout}"
    );
    assert!(
        stdout.contains("chat"),
        "cli banner should surface chat-shell rendering intent: {stdout}"
    );
}
