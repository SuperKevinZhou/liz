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
use std::path::{Path, PathBuf};
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
        let output = prepared.command.output().map_err(|error| {
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
        let command_process = build_shell_command(command, working_dir, &sandbox, &self.sandbox)?;
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
    fn new(command: String, working_dir: Option<String>, sandbox: EffectiveSandboxRequest) -> Self {
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
    config: &SandboxConfig,
) -> RuntimeResult<Command> {
    let mut built = match sandbox.mode {
        SandboxMode::DangerFullAccess | SandboxMode::ExternalSandbox => {
            direct_shell_command(command)
        }
        SandboxMode::ReadOnly | SandboxMode::WorkspaceWrite => {
            sandboxed_shell_command(command, working_dir, sandbox, config)?
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
    config: &SandboxConfig,
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
            let helper = helper_path_for_backend(sandbox.backend)?;
            Ok(build_linux_helper_command(
                &helper,
                command,
                working_dir,
                sandbox,
                config.linux_variant,
            ))
        }
        PlatformSandboxBackend::WindowsRestrictedToken => {
            let helper = helper_path_for_backend(sandbox.backend)?;
            Ok(build_windows_helper_command(&helper, command, working_dir, sandbox))
        }
        PlatformSandboxBackend::WindowsSandboxUser => {
            let helper = helper_path_for_backend(sandbox.backend)?;
            Ok(build_windows_helper_command(&helper, command, working_dir, sandbox))
        }
        PlatformSandboxBackend::None => Err(RuntimeError::invalid_state(
            "sandbox_backend_unavailable",
            "sandbox mode requires a platform backend",
        )),
    }
}

fn helper_path_for_backend(backend: PlatformSandboxBackend) -> RuntimeResult<String> {
    let (env_key, error_code, description) = match backend {
        PlatformSandboxBackend::LinuxHelper => {
            ("LIZ_LINUX_SANDBOX_HELPER", "linux_sandbox_helper_missing", "Linux sandbox helper")
        }
        PlatformSandboxBackend::WindowsRestrictedToken => (
            "LIZ_WINDOWS_RESTRICTED_TOKEN_HELPER",
            "windows_restricted_token_helper_missing",
            "Windows restricted-token sandbox helper",
        ),
        PlatformSandboxBackend::WindowsSandboxUser => (
            "LIZ_WINDOWS_SANDBOX_USER_HELPER",
            "windows_sandbox_user_helper_missing",
            "Windows sandbox-user helper",
        ),
        PlatformSandboxBackend::MacosSeatbelt | PlatformSandboxBackend::None => {
            return Err(RuntimeError::invalid_state(
                "sandbox_backend_helper_not_applicable",
                "helper path lookup is not applicable to this backend",
            ))
        }
    };
    if let Ok(value) = std::env::var(env_key) {
        return Ok(value);
    }

    resolve_default_helper_path(backend).ok_or_else(|| {
        RuntimeError::invalid_state(
            error_code,
            format!("{description} is required in {env_key} or on the default helper search path"),
        )
    })
}

fn build_linux_helper_command(
    helper: &str,
    command: &str,
    working_dir: Option<&str>,
    sandbox: &EffectiveSandboxRequest,
    linux_variant: crate::executor::LinuxSandboxVariant,
) -> Command {
    let mut command_process = Command::new(helper);
    command_process
        .arg("--sandbox-mode")
        .arg(sandbox.mode.as_str())
        .arg("--network-access")
        .arg(sandbox.network_access.as_str());
    if let Some(working_dir) = working_dir {
        command_process.arg("--working-dir").arg(working_dir);
    }
    if matches!(linux_variant, crate::executor::LinuxSandboxVariant::LegacyLandlock) {
        command_process.arg("--use-legacy-landlock");
    }
    command_process.arg("--").arg("sh").arg("-lc").arg(command);
    command_process
}

fn build_windows_helper_command(
    helper: &str,
    command: &str,
    working_dir: Option<&str>,
    sandbox: &EffectiveSandboxRequest,
) -> Command {
    let mut command_process = Command::new(helper);
    command_process
        .arg("--sandbox-mode")
        .arg(sandbox.mode.as_str())
        .arg("--network-access")
        .arg(sandbox.network_access.as_str());
    if let Some(working_dir) = working_dir {
        command_process.arg("--working-dir").arg(working_dir);
    }
    command_process.arg("--").arg("powershell").arg("-NoProfile").arg("-Command").arg(command);
    command_process
}

fn resolve_default_helper_path(backend: PlatformSandboxBackend) -> Option<String> {
    let names = helper_names_for_backend(backend)?;
    let mut candidates = Vec::new();
    if let Some(dir) = current_exe_dir() {
        candidates.extend(helper_candidates_in_dir(&dir, names));
    }
    for dir in path_dirs() {
        candidates.extend(helper_candidates_in_dir(&dir, names));
    }
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.to_string_lossy().to_string())
}

fn helper_names_for_backend(backend: PlatformSandboxBackend) -> Option<&'static [&'static str]> {
    match backend {
        PlatformSandboxBackend::LinuxHelper => {
            Some(&["liz-linux-sandbox", "liz-linux-sandbox-helper"])
        }
        PlatformSandboxBackend::WindowsRestrictedToken => {
            Some(&["liz-windows-restricted-token.exe", "liz-windows-restricted-token.cmd"])
        }
        PlatformSandboxBackend::WindowsSandboxUser => {
            Some(&["liz-windows-sandbox-user.exe", "liz-windows-sandbox-user.cmd"])
        }
        PlatformSandboxBackend::MacosSeatbelt | PlatformSandboxBackend::None => None,
    }
}

fn helper_candidates_in_dir(dir: &Path, names: &'static [&'static str]) -> Vec<PathBuf> {
    names.iter().map(|name| dir.join(name)).collect()
}

fn current_exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

fn path_dirs() -> impl Iterator<Item = PathBuf> {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
}

fn build_macos_policy(
    working_dir: Option<&str>,
    mode: SandboxMode,
    network_access: SandboxNetworkAccess,
) -> String {
    let workspace_root = working_dir.unwrap_or(".");
    let file_write_policy = if matches!(mode, SandboxMode::WorkspaceWrite) {
        format!("(allow file-write* (subpath \"{workspace_root}\"))\n")
    } else {
        String::new()
    };
    let network_policy = match network_access {
        SandboxNetworkAccess::Disabled => String::new(),
        SandboxNetworkAccess::Restricted => {
            "(allow network-outbound (remote ip \"localhost:*\"))\n".to_owned()
        }
        SandboxNetworkAccess::Enabled => {
            "(allow network-outbound)\n(allow network-inbound)\n".to_owned()
        }
    };

    format!(
        "(version 1)\n(deny default)\n(allow process*)\n(allow signal (target self))\n(allow file-read*)\n{file_write_policy}{network_policy}"
    )
}

fn decorate_stdout(sandbox: EffectiveSandboxRequest, stdout: &str) -> String {
    if matches!(sandbox.mode, SandboxMode::DangerFullAccess | SandboxMode::ExternalSandbox) {
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
        build_linux_helper_command, build_macos_policy, build_shell_command,
        build_windows_helper_command, helper_path_for_backend, sandboxed_shell_command,
        EffectiveSandboxRequest,
    };
    use crate::executor::{PlatformSandboxBackend, SandboxConfig, WindowsSandboxBackend};
    use liz_protocol::{SandboxMode, SandboxNetworkAccess, ShellSandboxRequest};
    use std::ffi::OsString;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    #[test]
    fn direct_modes_use_plain_shell_command() {
        let sandbox = EffectiveSandboxRequest {
            mode: SandboxMode::DangerFullAccess,
            network_access: SandboxNetworkAccess::Enabled,
            backend: PlatformSandboxBackend::None,
            request: None,
        };

        let command = build_shell_command("echo test", None, &sandbox, &SandboxConfig::default())
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

        let error = sandboxed_shell_command("Write-Output test", None, &effective, &config)
            .expect_err("unsupported backend should fail closed");
        assert_eq!(error.code(), "windows_sandbox_user_helper_missing");
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

    #[test]
    fn linux_helper_command_supports_legacy_landlock_flag() {
        let sandbox = EffectiveSandboxRequest {
            mode: SandboxMode::WorkspaceWrite,
            network_access: SandboxNetworkAccess::Restricted,
            backend: PlatformSandboxBackend::LinuxHelper,
            request: None,
        };

        let command = build_linux_helper_command(
            "/usr/local/bin/liz-linux-sandbox",
            "echo test",
            Some("/tmp/workspace"),
            &sandbox,
            crate::executor::LinuxSandboxVariant::LegacyLandlock,
        );
        let arguments =
            command.get_args().map(|value| value.to_string_lossy().to_string()).collect::<Vec<_>>();

        assert!(arguments.contains(&"--use-legacy-landlock".to_owned()));
        assert!(arguments.contains(&"--working-dir".to_owned()));
    }

    #[test]
    fn windows_helper_command_wraps_powershell_payload() {
        let sandbox = EffectiveSandboxRequest {
            mode: SandboxMode::WorkspaceWrite,
            network_access: SandboxNetworkAccess::Restricted,
            backend: PlatformSandboxBackend::WindowsRestrictedToken,
            request: None,
        };

        let command = build_windows_helper_command(
            "C:\\sandbox\\restricted-token.exe",
            "Write-Output test",
            Some("D:\\repo"),
            &sandbox,
        );
        let arguments =
            command.get_args().map(|value| value.to_string_lossy().to_string()).collect::<Vec<_>>();

        assert!(arguments.contains(&"powershell".to_owned()));
        assert!(arguments.contains(&"-Command".to_owned()));
        assert!(arguments.contains(&"Write-Output test".to_owned()));
    }

    #[test]
    fn helper_path_resolution_falls_back_to_default_search_path() {
        let _guard = env_lock().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let backend = if cfg!(target_os = "windows") {
            PlatformSandboxBackend::WindowsSandboxUser
        } else if cfg!(target_os = "linux") {
            PlatformSandboxBackend::LinuxHelper
        } else {
            return;
        };
        let temp_dir = TempDir::new().expect("temp dir should be created");
        let helper_name = if cfg!(target_os = "windows") {
            "liz-windows-sandbox-user.cmd"
        } else {
            "liz-linux-sandbox"
        };
        let helper_path = temp_dir.path().join(helper_name);
        fs::write(&helper_path, helper_stub_contents()).expect("helper stub should be written");

        let previous_path = std::env::var_os("PATH");
        let previous_override = helper_env_var_for_backend(backend).and_then(std::env::var_os);
        prepend_path(temp_dir.path());
        if let Some(env_key) = helper_env_var_for_backend(backend) {
            std::env::remove_var(env_key);
        }

        let resolved =
            helper_path_for_backend(backend).expect("default helper path should resolve");
        assert_eq!(PathBuf::from(resolved), helper_path);

        restore_path(previous_path);
        if let Some(env_key) = helper_env_var_for_backend(backend) {
            restore_optional_var(env_key, previous_override);
        }
    }

    fn helper_env_var_for_backend(backend: PlatformSandboxBackend) -> Option<&'static str> {
        match backend {
            PlatformSandboxBackend::LinuxHelper => Some("LIZ_LINUX_SANDBOX_HELPER"),
            PlatformSandboxBackend::WindowsRestrictedToken => {
                Some("LIZ_WINDOWS_RESTRICTED_TOKEN_HELPER")
            }
            PlatformSandboxBackend::WindowsSandboxUser => Some("LIZ_WINDOWS_SANDBOX_USER_HELPER"),
            PlatformSandboxBackend::MacosSeatbelt | PlatformSandboxBackend::None => None,
        }
    }

    fn helper_stub_contents() -> &'static str {
        if cfg!(target_os = "windows") {
            "@echo off\r\nexit /b 0\r\n"
        } else {
            "#!/bin/sh\nexit 0\n"
        }
    }

    fn prepend_path(dir: &Path) {
        let mut values = vec![dir.to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") {
            values.extend(std::env::split_paths(&path).collect::<Vec<_>>());
        }
        let joined = std::env::join_paths(values).expect("PATH should join");
        std::env::set_var("PATH", joined);
    }

    fn restore_path(value: Option<OsString>) {
        restore_optional_var("PATH", value);
    }

    fn restore_optional_var(key: &str, value: Option<OsString>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }
}
