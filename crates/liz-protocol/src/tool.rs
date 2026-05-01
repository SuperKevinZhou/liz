//! Tool-surface request and result payloads.

use crate::artifact::ArtifactRef;
use crate::ids::{ExecutorTaskId, NodeId, ThreadId, TurnId, WorkspaceMountId};
use crate::sandbox::{ShellSandboxRequest, ShellSandboxSummary};
use serde::{Deserialize, Serialize};

/// The stable tool names exposed by the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolName {
    /// Lists files and directories within a workspace root.
    WorkspaceList,
    /// Searches text inside workspace files.
    WorkspaceSearch,
    /// Reads a text file from the workspace.
    WorkspaceRead,
    /// Replaces the full contents of a text file.
    WorkspaceWriteText,
    /// Applies a bounded patch to a text file.
    WorkspaceApplyPatch,
    /// Runs one foreground shell command and waits for it to finish.
    ShellExec,
    /// Starts a background shell command.
    ShellSpawn,
    /// Waits for a background shell command to finish.
    ShellWait,
    /// Reads newly captured output from a background shell command.
    ShellReadOutput,
    /// Terminates a background shell command.
    ShellTerminate,
}

impl ToolName {
    /// Returns the canonical wire name for the tool.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WorkspaceList => "workspace.list",
            Self::WorkspaceSearch => "workspace.search",
            Self::WorkspaceRead => "workspace.read",
            Self::WorkspaceWriteText => "workspace.write_text",
            Self::WorkspaceApplyPatch => "workspace.apply_patch",
            Self::ShellExec => "shell.exec",
            Self::ShellSpawn => "shell.spawn",
            Self::ShellWait => "shell.wait",
            Self::ShellReadOutput => "shell.read_output",
            Self::ShellTerminate => "shell.terminate",
        }
    }
}

/// A typed tool invocation request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool_name", content = "input")]
pub enum ToolInvocation {
    /// Invokes `workspace.list`.
    #[serde(rename = "workspace.list")]
    WorkspaceList(WorkspaceListRequest),
    /// Invokes `workspace.search`.
    #[serde(rename = "workspace.search")]
    WorkspaceSearch(WorkspaceSearchRequest),
    /// Invokes `workspace.read`.
    #[serde(rename = "workspace.read")]
    WorkspaceRead(WorkspaceReadRequest),
    /// Invokes `workspace.write_text`.
    #[serde(rename = "workspace.write_text")]
    WorkspaceWriteText(WorkspaceWriteTextRequest),
    /// Invokes `workspace.apply_patch`.
    #[serde(rename = "workspace.apply_patch")]
    WorkspaceApplyPatch(WorkspaceApplyPatchRequest),
    /// Invokes `shell.exec`.
    #[serde(rename = "shell.exec")]
    ShellExec(ShellExecRequest),
    /// Invokes `shell.spawn`.
    #[serde(rename = "shell.spawn")]
    ShellSpawn(ShellSpawnRequest),
    /// Invokes `shell.wait`.
    #[serde(rename = "shell.wait")]
    ShellWait(ShellWaitRequest),
    /// Invokes `shell.read_output`.
    #[serde(rename = "shell.read_output")]
    ShellReadOutput(ShellReadOutputRequest),
    /// Invokes `shell.terminate`.
    #[serde(rename = "shell.terminate")]
    ShellTerminate(ShellTerminateRequest),
}

impl ToolInvocation {
    /// Returns the stable tool name for this invocation.
    pub const fn tool_name(&self) -> ToolName {
        match self {
            Self::WorkspaceList(_) => ToolName::WorkspaceList,
            Self::WorkspaceSearch(_) => ToolName::WorkspaceSearch,
            Self::WorkspaceRead(_) => ToolName::WorkspaceRead,
            Self::WorkspaceWriteText(_) => ToolName::WorkspaceWriteText,
            Self::WorkspaceApplyPatch(_) => ToolName::WorkspaceApplyPatch,
            Self::ShellExec(_) => ToolName::ShellExec,
            Self::ShellSpawn(_) => ToolName::ShellSpawn,
            Self::ShellWait(_) => ToolName::ShellWait,
            Self::ShellReadOutput(_) => ToolName::ShellReadOutput,
            Self::ShellTerminate(_) => ToolName::ShellTerminate,
        }
    }
}

/// A typed tool execution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool_name", content = "output")]
pub enum ToolResult {
    /// Result payload for `workspace.list`.
    #[serde(rename = "workspace.list")]
    WorkspaceList(WorkspaceListResult),
    /// Result payload for `workspace.search`.
    #[serde(rename = "workspace.search")]
    WorkspaceSearch(WorkspaceSearchResult),
    /// Result payload for `workspace.read`.
    #[serde(rename = "workspace.read")]
    WorkspaceRead(WorkspaceReadResult),
    /// Result payload for `workspace.write_text`.
    #[serde(rename = "workspace.write_text")]
    WorkspaceWriteText(WorkspaceWriteTextResult),
    /// Result payload for `workspace.apply_patch`.
    #[serde(rename = "workspace.apply_patch")]
    WorkspaceApplyPatch(WorkspaceApplyPatchResult),
    /// Result payload for `shell.exec`.
    #[serde(rename = "shell.exec")]
    ShellExec(ShellExecResult),
    /// Result payload for `shell.spawn`.
    #[serde(rename = "shell.spawn")]
    ShellSpawn(ShellSpawnResult),
    /// Result payload for `shell.wait`.
    #[serde(rename = "shell.wait")]
    ShellWait(ShellWaitResult),
    /// Result payload for `shell.read_output`.
    #[serde(rename = "shell.read_output")]
    ShellReadOutput(ShellReadOutputResult),
    /// Result payload for `shell.terminate`.
    #[serde(rename = "shell.terminate")]
    ShellTerminate(ShellTerminateResult),
}

impl ToolResult {
    /// Returns the stable tool name for this result.
    pub const fn tool_name(&self) -> ToolName {
        match self {
            Self::WorkspaceList(_) => ToolName::WorkspaceList,
            Self::WorkspaceSearch(_) => ToolName::WorkspaceSearch,
            Self::WorkspaceRead(_) => ToolName::WorkspaceRead,
            Self::WorkspaceWriteText(_) => ToolName::WorkspaceWriteText,
            Self::WorkspaceApplyPatch(_) => ToolName::WorkspaceApplyPatch,
            Self::ShellExec(_) => ToolName::ShellExec,
            Self::ShellSpawn(_) => ToolName::ShellSpawn,
            Self::ShellWait(_) => ToolName::ShellWait,
            Self::ShellReadOutput(_) => ToolName::ShellReadOutput,
            Self::ShellTerminate(_) => ToolName::ShellTerminate,
        }
    }
}

/// Executes a typed tool invocation inside one thread context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallRequest {
    /// The owning thread for tool traces and artifacts.
    pub thread_id: ThreadId,
    /// The turn this execution belongs to, if the call originated from a real turn.
    pub turn_id: Option<TurnId>,
    /// The node where this invocation should run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<NodeId>,
    /// The workspace mount used by this invocation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_mount_id: Option<WorkspaceMountId>,
    /// The typed tool invocation.
    #[serde(flatten)]
    pub invocation: ToolInvocation,
}

/// One file-system entry returned by `workspace.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceListEntry {
    /// The workspace-relative path for the entry.
    pub path: String,
    /// Whether the entry is a directory.
    pub is_dir: bool,
}

/// Input for `workspace.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceListRequest {
    /// The root directory to enumerate.
    pub root: String,
    /// Whether child directories should be traversed recursively.
    pub recursive: bool,
    /// Whether dot-prefixed or hidden entries should be included.
    pub include_hidden: bool,
    /// The maximum number of entries to return.
    pub max_entries: Option<usize>,
}

/// Output for `workspace.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceListResult {
    /// The root directory that was enumerated.
    pub root: String,
    /// Returned directory and file entries.
    pub entries: Vec<WorkspaceListEntry>,
    /// Whether the result was truncated by `max_entries`.
    pub truncated: bool,
}

/// Input for `workspace.search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSearchRequest {
    /// The directory to search below.
    pub root: String,
    /// The plain-text pattern to search for.
    pub pattern: String,
    /// Whether comparisons must match case exactly.
    pub case_sensitive: bool,
    /// Whether dot-prefixed or hidden entries should be included.
    pub include_hidden: bool,
    /// The maximum number of matches to return.
    pub max_results: Option<usize>,
}

/// One textual search hit returned by `workspace.search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSearchMatch {
    /// The workspace-relative file path containing the hit.
    pub path: String,
    /// The 1-based line number containing the hit.
    pub line_number: usize,
    /// The full matching line.
    pub line: String,
}

/// Output for `workspace.search`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceSearchResult {
    /// The root directory that was searched.
    pub root: String,
    /// The plain-text pattern that was searched.
    pub pattern: String,
    /// The matched lines.
    pub matches: Vec<WorkspaceSearchMatch>,
    /// Whether the result was truncated by `max_results`.
    pub truncated: bool,
}

/// Input for `workspace.read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceReadRequest {
    /// The file path to read.
    pub path: String,
    /// The first 1-based line to include.
    pub start_line: Option<usize>,
    /// The last 1-based line to include.
    pub end_line: Option<usize>,
}

/// Output for `workspace.read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceReadResult {
    /// The file path that was read.
    pub path: String,
    /// The extracted text content.
    pub content: String,
    /// The first line included in `content`.
    pub start_line: usize,
    /// The last line included in `content`.
    pub end_line: usize,
    /// The total number of lines in the file.
    pub total_lines: usize,
}

/// Input for `workspace.write_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWriteTextRequest {
    /// The file path to write.
    pub path: String,
    /// The full replacement contents for the file.
    pub content: String,
}

/// Output for `workspace.write_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceWriteTextResult {
    /// The file path that was written.
    pub path: String,
    /// Whether the contents changed.
    pub changed: bool,
    /// The number of bytes now stored in the file.
    pub byte_length: usize,
}

/// Input for `workspace.apply_patch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceApplyPatchRequest {
    /// The file path to patch.
    pub path: String,
    /// The exact text to search for.
    pub search: String,
    /// The replacement text to insert.
    pub replace: String,
    /// Whether every match should be replaced instead of only the first one.
    pub replace_all: bool,
}

/// Output for `workspace.apply_patch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceApplyPatchResult {
    /// The file path that was patched.
    pub path: String,
    /// The number of replacements that were applied.
    pub replacements: usize,
    /// Whether the file contents changed.
    pub changed: bool,
}

/// Input for `shell.exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellExecRequest {
    /// The shell command to run.
    pub command: String,
    /// The working directory for the command, if different from the process cwd.
    pub working_dir: Option<String>,
    /// Optional sandbox override for this command.
    pub sandbox: Option<ShellSandboxRequest>,
}

/// Output for `shell.exec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellExecResult {
    /// The command that was run.
    pub command: String,
    /// The working directory used for the command, if any.
    pub working_dir: Option<String>,
    /// The effective sandbox settings used for the command.
    pub sandbox: ShellSandboxSummary,
    /// The process exit code.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
}

/// Input for `shell.spawn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSpawnRequest {
    /// The shell command to run in the background.
    pub command: String,
    /// The working directory for the command, if different from the process cwd.
    pub working_dir: Option<String>,
    /// Optional sandbox override for this command.
    pub sandbox: Option<ShellSandboxRequest>,
}

/// Output for `shell.spawn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSpawnResult {
    /// The background executor task identifier.
    pub task_id: ExecutorTaskId,
    /// The command that was spawned.
    pub command: String,
    /// The working directory used for the command, if any.
    pub working_dir: Option<String>,
    /// The effective sandbox settings used for the command.
    pub sandbox: ShellSandboxSummary,
}

/// Input for `shell.wait`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellWaitRequest {
    /// The background task to wait for.
    pub task_id: ExecutorTaskId,
}

/// Output for `shell.wait`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellWaitResult {
    /// The background task that was waited on.
    pub task_id: ExecutorTaskId,
    /// The effective sandbox settings used for the task.
    pub sandbox: ShellSandboxSummary,
    /// Whether the command is still running after the wait.
    pub running: bool,
    /// The final exit code when available.
    pub exit_code: Option<i32>,
    /// The full accumulated standard output.
    pub stdout: String,
    /// The full accumulated standard error.
    pub stderr: String,
}

/// Input for `shell.read_output`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellReadOutputRequest {
    /// The background task whose output should be read.
    pub task_id: ExecutorTaskId,
}

/// Output for `shell.read_output`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellReadOutputResult {
    /// The background task whose output was read.
    pub task_id: ExecutorTaskId,
    /// The effective sandbox settings used for the task.
    pub sandbox: ShellSandboxSummary,
    /// Whether the command is still running.
    pub running: bool,
    /// The final exit code when available.
    pub exit_code: Option<i32>,
    /// Newly captured standard output since the last read.
    pub stdout_delta: String,
    /// Newly captured standard error since the last read.
    pub stderr_delta: String,
}

/// Input for `shell.terminate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellTerminateRequest {
    /// The background task to terminate.
    pub task_id: ExecutorTaskId,
}

/// Output for `shell.terminate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellTerminateResult {
    /// The background task that was terminated.
    pub task_id: ExecutorTaskId,
    /// The effective sandbox settings used for the task.
    pub sandbox: ShellSandboxSummary,
    /// Whether the terminate request sent a kill signal.
    pub terminated: bool,
    /// The observed exit code when available.
    pub exit_code: Option<i32>,
}

/// Response payload for one tool execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallResponse {
    /// The turn associated with this execution, synthesized when needed.
    pub execution_turn_id: TurnId,
    /// A short summary of what the tool returned.
    pub summary: String,
    /// The typed tool result.
    #[serde(flatten)]
    pub result: ToolResult,
    /// Artifacts created while executing the tool.
    pub artifact_refs: Vec<ArtifactRef>,
}
