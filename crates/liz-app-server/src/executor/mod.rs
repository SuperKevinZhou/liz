//! Tool-surface execution and normalization.

mod sandbox;
mod shell;
mod workspace;

use crate::runtime::RuntimeResult;
use liz_protocol::{
    ArtifactKind, ExecutorStream, ToolCallRequest, ToolInvocation, ToolName, ToolResult,
};
pub use sandbox::{
    EffectiveSandboxRequest, LinuxSandboxVariant, PlatformSandboxBackend, SandboxConfig,
    WindowsSandboxBackend,
};
use serde::Serialize;
use shell::LocalShellExecutor;

/// A persisted artifact payload produced while executing one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingArtifact {
    /// The artifact kind that should be stored.
    pub kind: ArtifactKind,
    /// A short human-readable summary.
    pub summary: String,
    /// The serialized artifact body.
    pub body: String,
}

/// The normalized outcome of executing one tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutedTool {
    /// The stable tool name that executed.
    pub tool_name: ToolName,
    /// A short completion summary.
    pub summary: String,
    /// The typed tool result payload.
    pub result: ToolResult,
    /// Artifact payloads that should be persisted.
    pub artifacts: Vec<PendingArtifact>,
    /// Normalized executor output chunks emitted while the tool ran.
    pub output_chunks: Vec<ExecutorOutput>,
}

/// One normalized executor output chunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutorOutput {
    /// The stream that emitted the chunk.
    pub stream: ExecutorStream,
    /// The emitted text chunk.
    pub chunk: String,
}

/// Dispatches typed tool invocations to the local runtime implementation.
#[derive(Debug, Clone, Default)]
pub struct ExecutorGateway {
    shell: std::sync::Arc<LocalShellExecutor>,
}

impl ExecutorGateway {
    /// Executes one typed tool invocation against the local workspace runtime.
    pub fn execute_tool(&self, request: &ToolCallRequest) -> RuntimeResult<ExecutedTool> {
        match &request.invocation {
            ToolInvocation::WorkspaceList(input) => {
                let result = workspace::list(input)?;
                let summary = format!(
                    "Listed {} workspace entries under {}",
                    result.entries.len(),
                    result.root
                );
                Ok(executed_tool(
                    ToolName::WorkspaceList,
                    summary,
                    ToolResult::WorkspaceList(result),
                    &request.invocation,
                    None,
                    Vec::new(),
                ))
            }
            ToolInvocation::WorkspaceSearch(input) => {
                let result = workspace::search(input)?;
                let summary = format!(
                    "Found {} matches for {} under {}",
                    result.matches.len(),
                    input.pattern,
                    result.root
                );
                Ok(executed_tool(
                    ToolName::WorkspaceSearch,
                    summary,
                    ToolResult::WorkspaceSearch(result),
                    &request.invocation,
                    None,
                    Vec::new(),
                ))
            }
            ToolInvocation::WorkspaceRead(input) => {
                let result = workspace::read(input)?;
                let summary = format!(
                    "Read lines {}-{} from {}",
                    result.start_line, result.end_line, result.path
                );
                Ok(executed_tool(
                    ToolName::WorkspaceRead,
                    summary,
                    ToolResult::WorkspaceRead(result),
                    &request.invocation,
                    None,
                    Vec::new(),
                ))
            }
            ToolInvocation::WorkspaceWriteText(input) => {
                let write = workspace::write_text(input)?;
                let summary = if write.result.changed {
                    format!("Wrote {} bytes to {}", write.result.byte_length, write.result.path)
                } else {
                    format!(
                        "Confirmed {} already matched the requested contents",
                        write.result.path
                    )
                };
                Ok(executed_tool(
                    ToolName::WorkspaceWriteText,
                    summary,
                    ToolResult::WorkspaceWriteText(write.result),
                    &request.invocation,
                    Some(diff_artifact(&input.path, &write.before, &write.after)),
                    Vec::new(),
                ))
            }
            ToolInvocation::WorkspaceApplyPatch(input) => {
                let patch = workspace::apply_patch(input)?;
                let summary = format!(
                    "Patched {} replacement(s) in {}",
                    patch.result.replacements, patch.result.path
                );
                Ok(executed_tool(
                    ToolName::WorkspaceApplyPatch,
                    summary,
                    ToolResult::WorkspaceApplyPatch(patch.result),
                    &request.invocation,
                    Some(diff_artifact(&input.path, &patch.before, &patch.after)),
                    Vec::new(),
                ))
            }
            ToolInvocation::ShellExec(input) => {
                let execution = self.shell.exec(input)?;
                let summary = format!(
                    "Command exited with code {}: {}",
                    execution.result.exit_code, execution.result.command
                );
                Ok(executed_tool(
                    ToolName::ShellExec,
                    summary,
                    ToolResult::ShellExec(execution.result.clone()),
                    &request.invocation,
                    Some(command_output_artifact(&execution.result)),
                    execution
                        .output_chunks
                        .into_iter()
                        .map(|chunk| ExecutorOutput { stream: chunk.stream, chunk: chunk.chunk })
                        .collect(),
                ))
            }
            ToolInvocation::ShellSpawn(input) => {
                let spawned = self.shell.spawn(input)?;
                let summary = format!("Spawned background shell task {}", spawned.task_id);
                Ok(executed_tool(
                    ToolName::ShellSpawn,
                    summary,
                    ToolResult::ShellSpawn(spawned),
                    &request.invocation,
                    None,
                    Vec::new(),
                ))
            }
            ToolInvocation::ShellReadOutput(input) => {
                let read = self.shell.read_output(input)?;
                let summary = format!("Read background shell output for {}", read.result.task_id);
                Ok(executed_tool(
                    ToolName::ShellReadOutput,
                    summary,
                    ToolResult::ShellReadOutput(read.result),
                    &request.invocation,
                    None,
                    read.output_chunks
                        .into_iter()
                        .map(|chunk| ExecutorOutput { stream: chunk.stream, chunk: chunk.chunk })
                        .collect(),
                ))
            }
            ToolInvocation::ShellWait(input) => {
                let waited = self.shell.wait(input)?;
                let summary = format!("Waited for background shell task {}", waited.result.task_id);
                Ok(executed_tool(
                    ToolName::ShellWait,
                    summary,
                    ToolResult::ShellWait(waited.result.clone()),
                    &request.invocation,
                    Some(command_output_artifact_from_wait(&waited.result)),
                    waited
                        .output_chunks
                        .into_iter()
                        .map(|chunk| ExecutorOutput { stream: chunk.stream, chunk: chunk.chunk })
                        .collect(),
                ))
            }
            ToolInvocation::ShellTerminate(input) => {
                let terminated = self.shell.terminate(input)?;
                let summary = format!("Terminated background shell task {}", terminated.task_id);
                Ok(executed_tool(
                    ToolName::ShellTerminate,
                    summary,
                    ToolResult::ShellTerminate(terminated),
                    &request.invocation,
                    None,
                    Vec::new(),
                ))
            }
        }
    }
}

fn executed_tool(
    tool_name: ToolName,
    summary: String,
    result: ToolResult,
    invocation: &ToolInvocation,
    extra_artifact: Option<PendingArtifact>,
    output_chunks: Vec<ExecutorOutput>,
) -> ExecutedTool {
    let snapshot_body = match &result {
        ToolResult::WorkspaceList(output) => serialize_snapshot(output),
        ToolResult::WorkspaceSearch(output) => serialize_snapshot(output),
        ToolResult::WorkspaceRead(output) => serialize_snapshot(output),
        ToolResult::WorkspaceWriteText(output) => serialize_snapshot(output),
        ToolResult::WorkspaceApplyPatch(output) => serialize_snapshot(output),
        ToolResult::ShellExec(output) => serialize_snapshot(output),
        ToolResult::ShellSpawn(output) => serialize_snapshot(output),
        ToolResult::ShellWait(output) => serialize_snapshot(output),
        ToolResult::ShellReadOutput(output) => serialize_snapshot(output),
        ToolResult::ShellTerminate(output) => serialize_snapshot(output),
    };
    let trace_body = serde_json::to_string_pretty(&ToolTraceArtifact {
        tool_name: tool_name.as_str().to_owned(),
        invocation,
        summary: summary.clone(),
        result: &result,
    })
    .expect("tool trace should serialize");

    let mut artifacts = vec![
        PendingArtifact {
            kind: ArtifactKind::ToolTrace,
            summary: format!("Tool trace for {}", tool_name.as_str()),
            body: trace_body,
        },
        PendingArtifact {
            kind: ArtifactKind::Snapshot,
            summary: summary.clone(),
            body: snapshot_body,
        },
    ];
    if let Some(extra_artifact) = extra_artifact {
        artifacts.push(extra_artifact);
    }

    ExecutedTool { tool_name, summary: summary.clone(), result, artifacts, output_chunks }
}

fn serialize_snapshot<T: Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).expect("tool snapshot should serialize")
}

#[derive(Debug, Serialize)]
struct ToolTraceArtifact<'a> {
    tool_name: String,
    invocation: &'a ToolInvocation,
    summary: String,
    result: &'a ToolResult,
}

fn diff_artifact(path: &str, before: &str, after: &str) -> PendingArtifact {
    PendingArtifact {
        kind: ArtifactKind::Diff,
        summary: format!("Diff for {path}"),
        body: render_diff(path, before, after),
    }
}

fn render_diff(path: &str, before: &str, after: &str) -> String {
    let mut diff = String::new();
    diff.push_str(&format!("--- {path}\n"));
    diff.push_str(&format!("+++ {path}\n"));
    diff.push_str("@@\n");
    for line in before.lines() {
        diff.push('-');
        diff.push_str(line);
        diff.push('\n');
    }
    for line in after.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

fn command_output_artifact(result: &liz_protocol::ShellExecResult) -> PendingArtifact {
    PendingArtifact {
        kind: ArtifactKind::CommandOutput,
        summary: format!("Command output for {}", result.command),
        body: serde_json::to_string_pretty(result).expect("command output should serialize"),
    }
}

fn command_output_artifact_from_wait(result: &liz_protocol::ShellWaitResult) -> PendingArtifact {
    PendingArtifact {
        kind: ArtifactKind::CommandOutput,
        summary: format!("Command output for {}", result.task_id),
        body: serde_json::to_string_pretty(result).expect("command output should serialize"),
    }
}
