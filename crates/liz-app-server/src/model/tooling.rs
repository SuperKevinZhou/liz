//! Provider-facing tool schema and continuation contracts.

use serde_json::json;
use std::collections::BTreeMap;

/// Tool-call protocol strategy used for one provider request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderToolProtocol {
    /// Use provider-native tool schema and tool-call/result primitives.
    Native,
    /// Use liz structured fallback for providers without reliable native tool APIs.
    StructuredFallback,
}

/// One provider-facing tool schema entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderToolSchema {
    /// Canonical liz runtime tool name, for example `workspace.read`.
    pub canonical_name: String,
    /// Provider-facing tool alias, for example `workspace_read`.
    pub provider_name: String,
    /// Human-readable tool summary shown to the model.
    pub description: String,
    /// JSON schema describing the tool input.
    pub input_json_schema: serde_json::Value,
}

/// Mapping between provider-facing tool aliases and canonical runtime tool names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolNameMap {
    alias_to_canonical: BTreeMap<String, String>,
    canonical_to_alias: BTreeMap<String, String>,
}

impl ProviderToolNameMap {
    /// Creates a name map from `(canonical, alias)` rows.
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let alias_to_canonical = pairs
            .iter()
            .map(|(canonical, alias)| (alias.to_string(), canonical.to_string()))
            .collect::<BTreeMap<_, _>>();
        let canonical_to_alias = pairs
            .iter()
            .map(|(canonical, alias)| (canonical.to_string(), alias.to_string()))
            .collect::<BTreeMap<_, _>>();

        Self { alias_to_canonical, canonical_to_alias }
    }

    /// Resolves a provider alias to canonical runtime name.
    pub fn canonical_name(&self, provider_name: &str) -> Option<&str> {
        self.alias_to_canonical.get(provider_name).map(String::as_str)
    }

    /// Resolves a canonical runtime name to provider alias.
    pub fn provider_name(&self, canonical_name: &str) -> Option<&str> {
        self.canonical_to_alias.get(canonical_name).map(String::as_str)
    }

    /// Returns all canonical-to-alias rows.
    pub fn pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.canonical_to_alias
            .iter()
            .map(|(canonical, alias)| (canonical.as_str(), alias.as_str()))
    }
}

/// The complete tool surface exposed to one model request.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolSurfaceSpec {
    /// Tool-call protocol strategy for this request.
    pub protocol: ProviderToolProtocol,
    /// Provider-facing tool schemas available to the model.
    pub tools: Vec<ProviderToolSchema>,
    /// Mapping between provider-facing and canonical tool names.
    pub name_map: ProviderToolNameMap,
}

impl ToolSurfaceSpec {
    /// Builds the standard liz runtime tool surface using the selected protocol.
    pub fn standard(protocol: ProviderToolProtocol) -> Self {
        let pairs = tool_name_pairs();
        let name_map = ProviderToolNameMap::from_pairs(&pairs);
        let tools = tool_schemas()
            .into_iter()
            .map(|schema| ProviderToolSchema {
                canonical_name: schema.canonical_name.to_string(),
                provider_name: name_map
                    .provider_name(schema.canonical_name)
                    .expect("tool alias should exist")
                    .to_string(),
                description: schema.description.to_string(),
                input_json_schema: schema.input_json_schema,
            })
            .collect();

        Self { protocol, tools, name_map }
    }

    /// Returns structured fallback instructions for providers without native tools.
    pub fn structured_fallback_instructions(&self) -> String {
        let mut sections = vec![
            "When you need runtime tools, emit exactly one <liz_tool_call> JSON object.".to_owned(),
            "Do not narrate the tool call in prose.".to_owned(),
            "Schema: {\"tool_name\":\"<provider_tool_name>\",\"arguments\":{...}}".to_owned(),
            "Allowed provider tool names:".to_owned(),
        ];
        for (canonical, alias) in self.name_map.pairs() {
            sections.push(format!("- {alias} (maps to {canonical})"));
        }
        sections.push(
            "Wait for a <liz_tool_result> block before deciding next step or claiming completion."
                .to_owned(),
        );
        sections.join("\n")
    }
}

/// A normalized provider tool call committed by model output.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderToolCall {
    /// Provider-scoped call identifier.
    pub call_id: String,
    /// Canonical liz runtime tool name.
    pub tool_name: String,
    /// Provider-facing alias used by the model.
    pub provider_tool_name: String,
    /// Parsed JSON arguments for tool execution.
    pub arguments: serde_json::Value,
}

/// One tool result injected back into the next provider round-trip.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolResultInjection {
    /// Provider-scoped call identifier.
    pub call_id: String,
    /// Canonical liz runtime tool name.
    pub tool_name: String,
    /// Provider-facing alias used in this provider request.
    pub provider_tool_name: String,
    /// Structured result payload for continuation.
    pub result: serde_json::Value,
    /// Whether tool execution failed.
    pub is_error: bool,
    /// Short runtime summary shown to the model.
    pub summary: String,
}

struct StaticToolSchema {
    canonical_name: &'static str,
    description: &'static str,
    input_json_schema: serde_json::Value,
}

fn tool_name_pairs() -> [(&'static str, &'static str); 10] {
    [
        ("workspace.list", "workspace_list"),
        ("workspace.search", "workspace_search"),
        ("workspace.read", "workspace_read"),
        ("workspace.write_text", "workspace_write_text"),
        ("workspace.apply_patch", "workspace_apply_patch"),
        ("shell.exec", "shell_exec"),
        ("shell.spawn", "shell_spawn"),
        ("shell.wait", "shell_wait"),
        ("shell.read_output", "shell_read_output"),
        ("shell.terminate", "shell_terminate"),
    ]
}

fn tool_schemas() -> Vec<StaticToolSchema> {
    vec![
        StaticToolSchema {
            canonical_name: "workspace.list",
            description: concat!(
                "List files and directories under an attached workspace path. Use this for structure discovery when you need to know what exists before searching or reading. ",
                "Prefer a narrow root over the workspace root when a likely directory is known. Use recursive=false for a quick top-level map, and recursive=true only when the subtree is small or max_entries is set. ",
                "Do not use this as a substitute for content search; use workspace.search when you need symbols, strings, or references. Output is structured entries with paths and directory/file metadata."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["root","recursive","include_hidden"],
                "properties":{
                    "root":{"type":"string"},
                    "recursive":{"type":"boolean"},
                    "include_hidden":{"type":"boolean"},
                    "max_entries":{"type":"integer","minimum":1}
                }
            }),
        },
        StaticToolSchema {
            canonical_name: "workspace.search",
            description: concat!(
                "Search text across attached workspace files. Use this before guessing file locations, symbols, configuration keys, tests, or call sites. ",
                "Start with the most specific stable term you know, then refine if results are noisy. Keep include_hidden=false unless hidden files are the real target. ",
                "Do not use broad generic terms that will flood the context; narrow by root or pattern instead. After finding likely matches, use workspace.read with line ranges before editing."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["root","pattern","case_sensitive","include_hidden"],
                "properties":{
                    "root":{"type":"string"},
                    "pattern":{"type":"string"},
                    "case_sensitive":{"type":"boolean"},
                    "include_hidden":{"type":"boolean"},
                    "max_results":{"type":"integer","minimum":1}
                }
            }),
        },
        StaticToolSchema {
            canonical_name: "workspace.read",
            description: concat!(
                "Read a file, optionally limited to a line range. Use this after listing or searching identifies relevant files. ",
                "For large files, always request the smallest useful start_line/end_line range and widen only if needed. Read before writing; do not patch files you have not inspected. ",
                "Use repeated focused reads instead of loading unrelated files into the main context. Output includes file content and line metadata suitable for citations and follow-up edits."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["path"],
                "properties":{
                    "path":{"type":"string"},
                    "start_line":{"type":"integer","minimum":1},
                    "end_line":{"type":"integer","minimum":1}
                }
            }),
        },
        StaticToolSchema {
            canonical_name: "workspace.write_text",
            description: concat!(
                "Replace an entire file with provided text. Use this for new files or intentional complete rewrites only. ",
                "Do not use it for surgical edits to existing files; use workspace.apply_patch so unrelated content is preserved. ",
                "Before replacing an existing file, read it first and be certain the full new content is correct. After writing, read back or run a focused verification."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["path","content"],
                "properties":{
                    "path":{"type":"string"},
                    "content":{"type":"string"}
                }
            }),
        },
        StaticToolSchema {
            canonical_name: "workspace.apply_patch",
            description: concat!(
                "Apply an exact search/replace patch to one workspace file. Use this for minimal, surgical edits after reading the target region. ",
                "The search text must match the existing file exactly. Keep the replacement scoped to the requested behavior and avoid opportunistic refactors. ",
                "Use replace_all only for deliberate repeated mechanical changes. If a patch fails, read the current file section again before trying a different patch."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["path","search","replace","replace_all"],
                "properties":{
                    "path":{"type":"string"},
                    "search":{"type":"string"},
                    "replace":{"type":"string"},
                    "replace_all":{"type":"boolean"}
                }
            }),
        },
        StaticToolSchema {
            canonical_name: "shell.exec",
            description: concat!(
                "Run one short foreground shell command and return exit code, stdout, and stderr. Use this for fast checks such as cargo test on a focused target, formatting verification, git status, or simple diagnostics. ",
                "Do not use it for long-running servers, watchers, or commands that need later output; use shell.spawn for those. ",
                "Prefer non-destructive commands. Check the exit code and reason from the captured output before claiming success."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["command"],
                "properties":{
                    "command":{"type":"string"},
                    "working_dir":{"type":"string"},
                    "sandbox":{
                        "type":"object",
                        "additionalProperties": false,
                        "required":["mode","network_access"],
                        "properties":{
                            "mode":{"type":"string"},
                            "network_access":{"type":"string"}
                        }
                    }
                }
            }),
        },
        StaticToolSchema {
            canonical_name: "shell.spawn",
            description: concat!(
                "Start a long-running shell process and return a task id. Use this for dev servers, watch mode tests, log tails, and commands that should keep running while liz continues the thread. ",
                "Do not use it for quick commands that should complete immediately; use shell.exec. ",
                "After spawning, use shell.read_output or shell.wait to observe progress, and shell.terminate when the process is no longer needed."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["command"],
                "properties":{
                    "command":{"type":"string"},
                    "working_dir":{"type":"string"},
                    "sandbox":{
                        "type":"object",
                        "additionalProperties": false,
                        "required":["mode","network_access"],
                        "properties":{
                            "mode":{"type":"string"},
                            "network_access":{"type":"string"}
                        }
                    }
                }
            }),
        },
        StaticToolSchema {
            canonical_name: "shell.wait",
            description: concat!(
                "Wait for a background shell task to finish and return its final status and buffered output. Use this when a spawned command is expected to complete soon. ",
                "Do not wait indefinitely on servers or watchers; use shell.read_output for snapshots and shell.terminate when appropriate. ",
                "Always inspect the final exit status before reporting verification success."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["task_id"],
                "properties":{"task_id":{"type":"string"}}
            }),
        },
        StaticToolSchema {
            canonical_name: "shell.read_output",
            description: concat!(
                "Read incremental stdout/stderr from a background shell task without necessarily stopping it. Use this to observe dev servers, watchers, and long-running diagnostics. ",
                "Use it after shell.spawn before deciding whether a process is healthy, blocked, or ready. ",
                "Do not infer success from silence; combine output, task state, and relevant follow-up checks."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["task_id"],
                "properties":{"task_id":{"type":"string"}}
            }),
        },
        StaticToolSchema {
            canonical_name: "shell.terminate",
            description: concat!(
                "Terminate a background shell task by task id. Use this when a dev server, watcher, or long-running command is no longer needed or is blocking progress. ",
                "Do not terminate unrelated tasks. Prefer reading recent output first so the final transcript explains why the process was stopped. ",
                "After termination, treat the result as a side effect and report any relevant output or cleanup needs."
            ),
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["task_id"],
                "properties":{"task_id":{"type":"string"}}
            }),
        },
    ]
}
