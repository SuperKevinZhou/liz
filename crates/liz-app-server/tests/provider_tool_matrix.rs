//! Provider tool-surface readiness and continuation coverage.

use liz_app_server::model::{
    ModelGateway, ModelGatewayConfig, ModelProviderFamily, ModelTurnRequest, ProviderToolProtocol,
    ProviderRegistry, ToolResultInjection, ToolSurfaceSpec,
};
use liz_protocol::{Thread, ThreadId, ThreadStatus, Timestamp, Turn, TurnId, TurnKind, TurnStatus};
use std::collections::BTreeMap;

#[test]
fn runtime_ready_providers_have_native_or_structured_tool_protocol() {
    let registry = ProviderRegistry::default();

    for provider in registry.providers().values().filter(|provider| provider.is_runtime_ready()) {
        let protocol = if matches!(provider.family, ModelProviderFamily::GitLabDuo) {
            ProviderToolProtocol::StructuredFallback
        } else if provider.capabilities.native_tool_calls {
            ProviderToolProtocol::Native
        } else {
            ProviderToolProtocol::StructuredFallback
        };
        let surface = ToolSurfaceSpec::standard(protocol);

        assert_eq!(surface.tools.len(), 10, "provider {} should expose 10 tools", provider.id);
        for tool in &surface.tools {
            assert!(
                surface.name_map.canonical_name(&tool.provider_name).is_some(),
                "provider {} produced unknown alias {}",
                provider.id,
                tool.provider_name,
            );
        }
    }
}

#[test]
fn openai_compatible_simulation_commits_tool_calls_with_provider_aliases() {
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "openrouter".to_owned(),
        overrides: BTreeMap::new(),
    })
    .with_simulation(true);

    let summary = gateway
        .run_turn(demo_request("run command: echo provider-tool"), |_| {})
        .expect("openrouter simulation should run");

    assert!(summary.assistant_message.is_none());
    assert_eq!(summary.tool_calls.len(), 1);
    assert_eq!(summary.tool_calls[0].tool_name, "shell.exec");
    assert_eq!(summary.tool_calls[0].provider_tool_name, "shell_exec");
}

#[test]
fn simulated_continuation_finishes_after_tool_result_injection() {
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "openrouter".to_owned(),
        overrides: BTreeMap::new(),
    })
    .with_simulation(true);

    let first = gateway
        .run_turn(demo_request("run command: echo continuation"), |_| {})
        .expect("first round should run");
    assert_eq!(first.tool_calls.len(), 1);
    assert!(first.assistant_message.is_none());

    let injection = ToolResultInjection {
        call_id: first.tool_calls[0].call_id.clone(),
        tool_name: first.tool_calls[0].tool_name.clone(),
        provider_tool_name: first.tool_calls[0].provider_tool_name.clone(),
        result: serde_json::json!({
            "tool_name":"shell.exec",
            "exit_code":0,
            "stdout":"continuation ok",
            "stderr":""
        }),
        is_error: false,
        summary: "shell.exec succeeded".to_owned(),
    };
    let second = gateway
        .run_turn(
            demo_request("run command: echo continuation")
                .with_tool_result_injections(vec![injection]),
            |_| {},
        )
        .expect("continuation round should run");

    assert!(second.tool_calls.is_empty());
    assert!(second.assistant_message.is_some());
}

fn demo_request(input: &str) -> ModelTurnRequest {
    ModelTurnRequest::from_prompt_parts(
        Thread {
            id: ThreadId::new("thread_tool_matrix"),
            title: "Tool Matrix".to_owned(),
            status: ThreadStatus::Active,
            created_at: Timestamp::new("2026-04-13T20:00:00Z"),
            updated_at: Timestamp::new("2026-04-13T20:00:00Z"),
            active_goal: Some("Validate provider tool matrix".to_owned()),
            active_summary: Some("Testing provider tool readiness".to_owned()),
            last_interruption: None,
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        },
        Turn {
            id: TurnId::new("turn_tool_matrix"),
            thread_id: ThreadId::new("thread_tool_matrix"),
            kind: TurnKind::User,
            status: TurnStatus::Running,
            started_at: Timestamp::new("2026-04-13T20:00:01Z"),
            ended_at: None,
            goal: Some("Validate tool continuation".to_owned()),
            summary: None,
            checkpoint_before: None,
            checkpoint_after: None,
        },
        "You are liz, a continuous personal agent.".to_owned(),
        "Use runtime context and tools to finish real work.".to_owned(),
        input.to_owned(),
    )
}

