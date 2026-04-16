//! Local shell-backed tool implementations.

use crate::executor::{EffectiveSandboxRequest, PlatformSandboxBackend, SandboxConfig};
use crate::runtime::{RuntimeError, RuntimeResult};
use liz_protocol::{
    ExecutorStream, ExecutorTaskId, SandboxMode, SandboxNetworkAccess, ShellExecRequest,
    ShellExecResult, ShellReadOutputRequest, ShellReadOutputResult, ShellSpawnRequest,
    ShellSpawnResult, ShellTerminateRequest, ShellTerminateResult, ShellWaitRequest,
    ShellWaitResult,
};
use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// Manages foreground and background shell execution for the local runtime.
#[derive(Debug)]
pub struct LocalShellExecutor {
    tasks: Mutex<HashMap<ExecutorTaskId, BackgroundShellTask>>,
    sequence: AtomicU64,
    sandbox: SandboxConfig,
}

impl Default for LocalShellExecutor {
    fn default() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
            sequence: AtomicU64::new(0),
            sandbox: SandboxConfig::default(),
        }
    }
}

impl LocalShellExecutor {
    /// Executes one shell command and waits for it to finish.
    pub fn exec(&self, request: &ShellExecRequest) -> RuntimeResult<ShellExecution> {
        let mut prepared = self.prepare_shell_command(
            &request.command,
            request.working_dir.as_deref(),
            request.sandbox.as_ref(),
        )?;
        let output = prepared
            .command
            .output()
            .map_err(|error| {
                RuntimeError::invalid_state(
                    "shell_exec_failed",
                    format!("failed to execute shell command: {error}"),
                )
            })?;
        let decorated_stdout =
            decorate_stdout(prepared.sandbox.clone(), &String::from_utf8_lossy(&output.stdout));

        Ok(ShellExecution {
            result: ShellExecResult {
                command: request.command.clone(),
                working_dir: request.working_dir.clone(),
                sandbox: prepared.sandbox.to_summary(),
                exit_code: output.status.code().unwrap_or(-1),
                stdout: decorated_stdout.clone(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            },
            output_chunks: output_chunks(
                &decorated_stdout,
                &String::from_utf8_lossy(&output.stderr),
            ),
        })
    }

    /// Starts a shell command in the background and returns its task identifier.
    pub fn spawn(&self, request: &ShellSpawnRequest) -> RuntimeResult<ShellSpawnResult> {
        let mut prepared = self.prepare_shell_command(
            &request.command,
            request.working_dir.as_deref(),
            request.sandbox.as_ref(),
        )?;
        let mut child = prepared.command.spawn().map_err(|error| {
            RuntimeError::invalid_state(
                "shell_spawn_failed",
                format!("failed to spawn shell command: {error}"),
            )
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            RuntimeError::invalid_state(
                "shell_stdout_missing",
                "background shell stdout was not captured",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            RuntimeError::invalid_state(
                "shell_stderr_missing",
                "background shell stderr was not captured",
            )
        })?;

        let task_id = self.next_task_id();
        let sandbox_summary = prepared.sandbox.to_summary();
        let state = Arc::new(Mutex::new(BackgroundShellState::new(
            request.command.clone(),
            request.working_dir.clone(),
            prepared.sandbox,
        )));
        start_reader(stdout, ExecutorStream::Stdout, Arc::clone(&state));
        start_reader(stderr, ExecutorStream::Stderr, Arc::clone(&state));

        self.tasks
            .lock()
            .expect("background shell task mutex should not be poisoned")
            .insert(task_id.clone(), BackgroundShellTask { child: Mutex::new(child), state });

        Ok(ShellSpawnResult {
            task_id,
            command: request.command.clone(),
            working_dir: request.working_dir.clone(),
            sandbox: sandbox_summary,
        })
    }

    /// Reads newly captured output from a background shell task.
    pub fn read_output(
        &self,
        request: &ShellReadOutputRequest,
    ) -> RuntimeResult<ShellReadOutputExecution> {
        let tasks = self.tasks.lock().expect("background shell task mutex should not be poisoned");
        let task = tasks.get(&request.task_id).ok_or_else(|| {
            RuntimeError::not_found("shell_task_not_found", "background shell task does not exist")
        })?;
        update_exit_code_if_finished(task);
        let mut state =
            task.state.lock().expect("background shell state mutex should not be poisoned");

        let stdout_delta = state.stdout[state.delivered_stdout_len..].to_owned();
        let stderr_delta = state.stderr[state.delivered_stderr_len..].to_owned();
        state.delivered_stdout_len = state.stdout.len();
        state.delivered_stderr_len = state.stderr.len();

        Ok(ShellReadOutputExecution {
            result: ShellReadOutputResult {
                task_id: request.task_id.clone(),
                sandbox: state.sandbox.to_summary(),
                running: state.exit_code.is_none(),
                exit_code: state.exit_code,
                stdout_delta: stdout_delta.clone(),
                stderr_delta: stderr_delta.clone(),
            },
            output_chunks: output_chunks(&stdout_delta, &stderr_delta),
        })
    }

    /// Waits for a background shell task to finish and returns its final output.
    pub fn wait(&self, request: &ShellWaitRequest) -> RuntimeResult<ShellWaitExecution> {
        let tasks = self.tasks.lock().expect("background shell task mutex should not be poisoned");
        let task = tasks.get(&request.task_id).ok_or_else(|| {
            RuntimeError::not_found("shell_task_not_found", "background shell task does not exist")
        })?;

        {
            let mut child =
                task.child.lock().expect("background shell child mutex should not be poisoned");
            if let Some(exit_code) = wait_for_exit_code(&mut child)? {
                let mut state =
                    task.state.lock().expect("background shell state mutex should not be poisoned");
                state.exit_code = Some(exit_code);
            }
        }

        let state = task.state.lock().expect("background shell state mutex should not be poisoned");

        Ok(ShellWaitExecution {
            result: ShellWaitResult {
                task_id: request.task_id.clone(),
                sandbox: state.sandbox.to_summary(),
                running: state.exit_code.is_none(),
                exit_code: state.exit_code,
                stdout: state.stdout.clone(),
                stderr: state.stderr.clone(),
            },
            output_chunks: output_chunks(&state.stdout, &state.stderr),
        })
    }

    /// Terminates a background shell task.
    pub fn terminate(
        &self,
        request: &ShellTerminateRequest,
    ) -> RuntimeResult<ShellTerminateResult> {
        let tasks = self.tasks.lock().expect("background shell task mutex should not be poisoned");
        let task = tasks.get(&request.task_id).ok_or_else(|| {
            RuntimeError::not_found("shell_task_not_found", "background shell task does not exist")
        })?;

        let exit_code = {
            let mut child =
                task.child.lock().expect("background shell child mutex should not be poisoned");
            let _ = child.kill();
            wait_for_exit_code(&mut child)?
        };

        let mut state =
            task.state.lock().expect("background shell state mutex should not be poisoned");
        state.exit_code = exit_code.or(state.exit_code);

        Ok(ShellTerminateResult {
            task_id: request.task_id.clone(),
            sandbox: state.sandbox.to_summary(),
            terminated: true,
            exit_code: state.exit_code,
        })
    }

    fn next_task_id(&self) -> ExecutorTaskId {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        ExecutorTaskId::new(format!("executor_{:x}_{sequence}", now.as_nanos()))
    }

    fn prepare_shell_command(
        &self,
        command: &str,
        working_dir: Option<&str>,
        sandbox_override: Option<&liz_protocol::ShellSandboxRequest>,
    ) -> RuntimeResult<PreparedShellCommand> {
        let sandbox = self.sandbox.resolve_request(sandbox_override);
        sandbox.ensure_supported()?;
        let command_process = build_shell_command(command, working_dir, &sandbox)?;
        Ok(PreparedShellCommand { command: command_process, sandbox })
    }
}

/// The normalized result of one foreground shell execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellExecution {
    /// The structured shell execution result.
    pub result: ShellExecResult,
    /// Output chunks captured from stdout and stderr.
    pub output_chunks: Vec<ShellOutputChunk>,
}

/// The normalized result of reading background shell output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellReadOutputExecution {
    /// The structured read-output result.
    pub result: ShellReadOutputResult,
    /// Output chunks captured since the last read.
    pub output_chunks: Vec<ShellOutputChunk>,
}

/// The normalized result of waiting on a background shell task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellWaitExecution {
    /// The structured wait result.
    pub result: ShellWaitResult,
    /// The full accumulated output observed while waiting.
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

#[derive(Debug)]
struct BackgroundShellTask {
    child: Mutex<Child>,
    state: Arc<Mutex<BackgroundShellState>>,
}

#[derive(Debug)]
struct BackgroundShellState {
    stdout: String,
    stderr: String,
    delivered_stdout_len: usize,
    delivered_stderr_len: usize,
    exit_code: Option<i32>,
    sandbox: EffectiveSandboxRequest,
    _command: String,
    _working_dir: Option<String>,
}

impl BackgroundShellState {
    fn new(
        command: String,
        working_dir: Option<String>,
        sandbox: EffectiveSandboxRequest,
    ) -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
            delivered_stdout_len: 0,
            delivered_stderr_len: 0,
            exit_code: None,
            sandbox,
            _command: command,
            _working_dir: working_dir,
        }
    }
}

#[derive(Debug)]
struct PreparedShellCommand {
    command: Command,
    sandbox: EffectiveSandboxRequest,
}

fn build_shell_command(
    command: &str,
    working_dir: Option<&str>,
    sandbox: &EffectiveSandboxRequest,
) -> RuntimeResult<Command> {
    let mut built = match sandbox.mode {
        SandboxMode::DangerFullAccess | SandboxMode::ExternalSandbox => direct_shell_command(command),
        SandboxMode::ReadOnly | SandboxMode::WorkspaceWrite => {
            sandboxed_shell_command(command, working_dir, sandbox)?
        }
    };

    built.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(working_dir) = working_dir {
        built.current_dir(working_dir);
    }
    Ok(built)
}

fn direct_shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        let mut command_process = Command::new("powershell");
        command_process.arg("-NoProfile").arg("-Command").arg(command);
        command_process
    }

    #[cfg(not(windows))]
    {
        let mut command_process = Command::new("sh");
        command_process.arg("-lc").arg(command);
        command_process
    }
}

fn sandboxed_shell_command(
    command: &str,
    working_dir: Option<&str>,
    sandbox: &EffectiveSandboxRequest,
) -> RuntimeResult<Command> {
    match sandbox.backend {
        PlatformSandboxBackend::MacosSeatbelt => {
            let mut command_process = Command::new("/usr/bin/sandbox-exec");
            command_process
                .arg("-p")
                .arg(build_macos_policy(working_dir, sandbox.mode, sandbox.network_access))
                .arg("--")
                .arg("sh")
                .arg("-lc")
                .arg(command);
            Ok(command_process)
        }
        PlatformSandboxBackend::LinuxHelper => {
            let helper = std::env::var("LIZ_LINUX_SANDBOX_HELPER").map_err(|_| {
                RuntimeError::invalid_state(
                    "linux_sandbox_helper_missing",
                    "sandbox mode requires LIZ_LINUX_SANDBOX_HELPER to point at the Linux sandbox helper",
                )
            })?;
            let mut command_process = Command::new(helper);
            command_process
                .arg("--sandbox-mode")
                .arg(sandbox.mode.as_str())
                .arg("--network-access")
                .arg(sandbox.network_access.as_str());
            if let Some(working_dir) = working_dir {
                command_process.arg("--working-dir").arg(working_dir);
            }
            command_process.arg("--").arg("sh").arg("-lc").arg(command);
            Ok(command_process)
        }
        PlatformSandboxBackend::WindowsRestrictedToken => Err(RuntimeError::unsupported(
            "windows_restricted_token_unimplemented",
            "windows restricted-token sandbox execution is not implemented yet",
        )),
        PlatformSandboxBackend::WindowsSandboxUser => Err(RuntimeError::unsupported(
            "windows_sandbox_user_unimplemented",
            "windows sandbox-user execution is not implemented yet",
        )),
        PlatformSandboxBackend::None => Err(RuntimeError::invalid_state(
            "sandbox_backend_unavailable",
            "sandbox mode requires a platform backend",
        )),
    }
}

fn build_macos_policy(
    working_dir: Option<&str>,
    mode: SandboxMode,
    network_access: SandboxNetworkAccess,
) -> String {
    let workspace_root = working_dir.unwrap_or(".");
    let file_write_policy = if matches!(mode, SandboxMode::WorkspaceWrite) {
        format!(
            "(allow file-write* (subpath \"{workspace_root}\"))\n"
        )
    } else {
        String::new()
    };
    let network_policy = match network_access {
        SandboxNetworkAccess::Disabled => String::new(),
        SandboxNetworkAccess::Restricted => {
            "(allow network-outbound (remote ip \"localhost:*\"))\n".to_owned()
        }
        SandboxNetworkAccess::Enabled => "(allow network-outbound)\n(allow network-inbound)\n".to_owned(),
    };

    format!(
        "(version 1)\n(deny default)\n(allow process*)\n(allow signal (target self))\n(allow file-read*)\n{file_write_policy}{network_policy}"
    )
}

fn decorate_stdout(sandbox: EffectiveSandboxRequest, stdout: &str) -> String {
    if matches!(
        sandbox.mode,
        SandboxMode::DangerFullAccess | SandboxMode::ExternalSandbox
    ) {
        stdout.to_owned()
    } else {
        format!(
            "[sandbox mode={} backend={} network={}]\n{}",
            sandbox.mode.as_str(),
            sandbox.backend.as_str(),
            sandbox.network_access.as_str(),
            stdout
        )
    }
}

fn start_reader<R>(mut reader: R, stream: ExecutorStream, state: Arc<Mutex<BackgroundShellState>>)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    let chunk = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let mut state =
                        state.lock().expect("background shell state mutex should not be poisoned");
                    match stream {
                        ExecutorStream::Stdout => {
                            if state.stdout.is_empty()
                                && !matches!(
                                    state.sandbox.mode,
                                    SandboxMode::DangerFullAccess | SandboxMode::ExternalSandbox
                                )
                            {
                                let banner = format!(
                                    "[sandbox mode={} backend={} network={}]\n",
                                    state.sandbox.mode.as_str(),
                                    state.sandbox.backend.as_str(),
                                    state.sandbox.network_access.as_str()
                                );
                                state.stdout.push_str(&banner);
                            }
                            state.stdout.push_str(&chunk);
                        }
                        ExecutorStream::Stderr => state.stderr.push_str(&chunk),
                    }
                }
                Err(_) => break,
            }
        }
    });
}

fn output_chunks(stdout: &str, stderr: &str) -> Vec<ShellOutputChunk> {
    let mut chunks = Vec::new();
    if !stdout.is_empty() {
        chunks.push(ShellOutputChunk { stream: ExecutorStream::Stdout, chunk: stdout.to_owned() });
    }
    if !stderr.is_empty() {
        chunks.push(ShellOutputChunk { stream: ExecutorStream::Stderr, chunk: stderr.to_owned() });
    }
    chunks
}

fn update_exit_code_if_finished(task: &BackgroundShellTask) {
    let mut child = task.child.lock().expect("background shell child mutex should not be poisoned");
    let Ok(Some(status)) = child.try_wait() else {
        return;
    };
    let mut state = task.state.lock().expect("background shell state mutex should not be poisoned");
    state.exit_code = Some(status.code().unwrap_or(-1));
}

fn wait_for_exit_code(child: &mut Child) -> RuntimeResult<Option<i32>> {
    let status = child.wait().map_err(|error| {
        RuntimeError::invalid_state(
            "shell_wait_failed",
            format!("failed while waiting for shell command: {error}"),
        )
    })?;
    Ok(Some(status.code().unwrap_or(-1)))
}

#[cfg(test)]
mod tests {
    use super::{
        build_macos_policy, build_shell_command, sandboxed_shell_command, EffectiveSandboxRequest,
    };
    use crate::executor::{PlatformSandboxBackend, SandboxConfig, WindowsSandboxBackend};
    use liz_protocol::{SandboxMode, SandboxNetworkAccess, ShellSandboxRequest};

    #[test]
    fn direct_modes_use_plain_shell_command() {
        let sandbox = EffectiveSandboxRequest {
            mode: SandboxMode::DangerFullAccess,
            network_access: SandboxNetworkAccess::Enabled,
            backend: PlatformSandboxBackend::None,
            request: None,
        };

        let command = build_shell_command("echo test", None, &sandbox)
            .expect("danger-full-access command should build");
        let program = command.get_program().to_string_lossy().to_ascii_lowercase();
        assert!(program.contains("powershell") || program == "sh");
    }

    #[test]
    fn windows_sandbox_modes_fail_closed_without_backend_support() {
        if !cfg!(target_os = "windows") {
            return;
        }

        let config = SandboxConfig {
            default_mode: SandboxMode::WorkspaceWrite,
            default_network_access: SandboxNetworkAccess::Restricted,
            windows_backend: WindowsSandboxBackend::SandboxUser,
            linux_variant: crate::executor::LinuxSandboxVariant::Helper,
        };
        let effective = config.resolve_request(Some(&ShellSandboxRequest {
            mode: SandboxMode::WorkspaceWrite,
            network_access: SandboxNetworkAccess::Restricted,
        }));

        let error = sandboxed_shell_command("Write-Output test", None, &effective)
            .expect_err("unsupported backend should fail closed");
        assert_eq!(error.code(), "windows_sandbox_user_unimplemented");
    }

    #[test]
    fn macos_policy_allows_workspace_write_and_restricted_network() {
        let policy = build_macos_policy(
            Some("/tmp/workspace"),
            SandboxMode::WorkspaceWrite,
            SandboxNetworkAccess::Restricted,
        );

        assert!(policy.contains("(allow file-write* (subpath \"/tmp/workspace\"))"));
        assert!(policy.contains("(allow network-outbound (remote ip \"localhost:*\"))"));
    }
}
