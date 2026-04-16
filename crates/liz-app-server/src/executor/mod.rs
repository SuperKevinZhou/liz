//! Tool-surface execution and normalization.

mod workspace;

use crate::runtime::RuntimeResult;
use liz_protocol::{ArtifactKind, ToolCallRequest, ToolInvocation, ToolName, ToolResult};
use serde::Serialize;

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
}

/// Dispatches typed tool invocations to the local runtime implementation.
#[derive(Debug, Clone, Default)]
pub struct ExecutorGateway;

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
) -> ExecutedTool {
    let snapshot_body = match &result {
        ToolResult::WorkspaceList(output) => serialize_snapshot(output),
        ToolResult::WorkspaceSearch(output) => serialize_snapshot(output),
        ToolResult::WorkspaceRead(output) => serialize_snapshot(output),
    };
    let trace_body = serde_json::to_string_pretty(&ToolTraceArtifact {
        tool_name: tool_name.as_str().to_owned(),
        invocation,
        summary: summary.clone(),
        result: &result,
    })
    .expect("tool trace should serialize");

    ExecutedTool {
        tool_name,
        summary: summary.clone(),
        result,
        artifacts: vec![
            PendingArtifact {
                kind: ArtifactKind::ToolTrace,
                summary: format!("Tool trace for {}", tool_name.as_str()),
                body: trace_body,
            },
            PendingArtifact { kind: ArtifactKind::Snapshot, summary, body: snapshot_body },
        ],
    }
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
