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
            description: "List files and directories in a workspace root.",
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
            description: "Search plain text across workspace files.",
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
            description: "Read a file or file line range from the workspace.",
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
            description: "Replace an entire file with provided content.",
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
            description: "Apply exact search/replace patch on a file.",
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
            description: "Run one foreground shell command and return its output.",
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
            description: "Spawn a background shell command.",
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
            description: "Wait for a background shell task.",
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["task_id"],
                "properties":{"task_id":{"type":"string"}}
            }),
        },
        StaticToolSchema {
            canonical_name: "shell.read_output",
            description: "Read incremental output from a background shell task.",
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["task_id"],
                "properties":{"task_id":{"type":"string"}}
            }),
        },
        StaticToolSchema {
            canonical_name: "shell.terminate",
            description: "Terminate a background shell task.",
            input_json_schema: json!({
                "type":"object",
                "additionalProperties": false,
                "required":["task_id"],
                "properties":{"task_id":{"type":"string"}}
            }),
        },
    ]
}
