//! Local shell-backed tool implementations.

use crate::runtime::{RuntimeError, RuntimeResult};
use liz_protocol::{ExecutorStream, ShellExecRequest, ShellExecResult};
use std::process::{Command, Stdio};

/// The normalized result of one foreground shell execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExecution {
    /// The structured shell execution result.
    pub result: ShellExecResult,
    /// Output chunks captured from stdout and stderr.
    pub output_chunks: Vec<ShellOutputChunk>,
}

/// One normalized shell output chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellOutputChunk {
    /// The stream that emitted the chunk.
    pub stream: ExecutorStream,
    /// The text chunk emitted by the shell process.
    pub chunk: String,
}

/// Executes one shell command and waits for it to finish.
pub fn exec(request: &ShellExecRequest) -> RuntimeResult<ShellExecution> {
    #[cfg(windows)]
    let mut command = {
        let mut command = Command::new("powershell");
        command.arg("-NoProfile").arg("-Command").arg(&request.command);
        command
    };

    #[cfg(not(windows))]
    let mut command = {
        let mut command = Command::new("sh");
        command.arg("-lc").arg(&request.command);
        command
    };

    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

    if let Some(working_dir) = &request.working_dir {
        command.current_dir(working_dir);
    }

    let output = command.output().map_err(|error| {
        RuntimeError::invalid_state(
            "shell_exec_failed",
            format!("failed to execute shell command: {error}"),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let mut output_chunks = Vec::new();
    if !stdout.is_empty() {
        output_chunks
            .push(ShellOutputChunk { stream: ExecutorStream::Stdout, chunk: stdout.clone() });
    }
    if !stderr.is_empty() {
        output_chunks
            .push(ShellOutputChunk { stream: ExecutorStream::Stderr, chunk: stderr.clone() });
    }

    Ok(ShellExecution {
        result: ShellExecResult {
            command: request.command.clone(),
            working_dir: request.working_dir.clone(),
            exit_code: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
        },
        output_chunks,
    })
}
