//! Local shell-backed tool implementations.

use crate::runtime::{RuntimeError, RuntimeResult};
use liz_protocol::{
    ExecutorStream, ExecutorTaskId, ShellExecRequest, ShellExecResult, ShellReadOutputRequest,
    ShellReadOutputResult, ShellSpawnRequest, ShellSpawnResult, ShellTerminateRequest,
    ShellTerminateResult, ShellWaitRequest, ShellWaitResult,
};
use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// Manages foreground and background shell execution for the local runtime.
#[derive(Debug, Default)]
pub struct LocalShellExecutor {
    tasks: Mutex<HashMap<ExecutorTaskId, BackgroundShellTask>>,
    sequence: AtomicU64,
}

impl LocalShellExecutor {
    /// Executes one shell command and waits for it to finish.
    pub fn exec(&self, request: &ShellExecRequest) -> RuntimeResult<ShellExecution> {
        let output = build_shell_command(&request.command, request.working_dir.as_deref())?
            .output()
            .map_err(|error| {
                RuntimeError::invalid_state(
                    "shell_exec_failed",
                    format!("failed to execute shell command: {error}"),
                )
            })?;

        Ok(ShellExecution {
            result: ShellExecResult {
                command: request.command.clone(),
                working_dir: request.working_dir.clone(),
                exit_code: output.status.code().unwrap_or(-1),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            },
            output_chunks: output_chunks(
                &String::from_utf8_lossy(&output.stdout),
                &String::from_utf8_lossy(&output.stderr),
            ),
        })
    }

    /// Starts a shell command in the background and returns its task identifier.
    pub fn spawn(&self, request: &ShellSpawnRequest) -> RuntimeResult<ShellSpawnResult> {
        let mut command = build_shell_command(&request.command, request.working_dir.as_deref())?;
        let mut child = command.spawn().map_err(|error| {
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
        let state = Arc::new(Mutex::new(BackgroundShellState::new(
            request.command.clone(),
            request.working_dir.clone(),
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
            terminated: true,
            exit_code: state.exit_code,
        })
    }

    fn next_task_id(&self) -> ExecutorTaskId {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed) + 1;
        ExecutorTaskId::new(format!("executor_{:x}_{sequence}", now.as_nanos()))
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
    _command: String,
    _working_dir: Option<String>,
}

impl BackgroundShellState {
    fn new(command: String, working_dir: Option<String>) -> Self {
        Self {
            stdout: String::new(),
            stderr: String::new(),
            delivered_stdout_len: 0,
            delivered_stderr_len: 0,
            exit_code: None,
            _command: command,
            _working_dir: working_dir,
        }
    }
}

fn build_shell_command(command: &str, working_dir: Option<&str>) -> RuntimeResult<Command> {
    #[cfg(windows)]
    let mut built = {
        let mut command_process = Command::new("powershell");
        command_process.arg("-NoProfile").arg("-Command").arg(command);
        command_process
    };

    #[cfg(not(windows))]
    let mut built = {
        let mut command_process = Command::new("sh");
        command_process.arg("-lc").arg(command);
        command_process
    };

    built.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(working_dir) = working_dir {
        built.current_dir(working_dir);
    }
    Ok(built)
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
                        ExecutorStream::Stdout => state.stdout.push_str(&chunk),
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
