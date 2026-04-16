//! Provider-registry and provider-family plan coverage.

use liz_app_server::model::{
    ModelGateway, ModelGatewayConfig, ModelProviderFamily, ModelTurnRequest, NormalizedTurnEvent,
    ProviderOverride, ProviderRegistry,
};
use liz_protocol::{Thread, ThreadId, ThreadStatus, Timestamp, Turn, TurnId, TurnKind, TurnStatus};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

#[test]
fn registry_contains_all_llm_provider_ids_we_expect() {
    let registry = ProviderRegistry::default();
    let ids = registry.supported_provider_ids();

    for expected in [
        "openai",
        "anthropic",
        "google",
        "google-vertex",
        "google-vertex-anthropic",
        "anthropic-vertex",
        "amazon-bedrock",
        "github-copilot",
        "gitlab",
        "openrouter",
        "ollama",
        "vllm",
        "sglang",
        "opencode",
        "opencode-go",
        "qwen",
        "zai",
        "vercel-ai-gateway",
        "cloudflare-ai-gateway",
        "cloudflare-workers-ai",
        "sap-ai-core",
    ] {
        assert!(
            ids.contains(&expected),
            "expected provider {expected} to exist in builtin registry"
        );
    }
}

#[test]
fn provider_families_match_expected_transport_groups() {
    let registry = ProviderRegistry::default();

    assert_eq!(
        registry.provider("openai").expect("openai spec").family,
        ModelProviderFamily::OpenAiResponses
    );
    assert_eq!(
        registry.provider("anthropic").expect("anthropic spec").family,
        ModelProviderFamily::AnthropicMessages
    );
    assert_eq!(
        registry.provider("google").expect("google spec").family,
        ModelProviderFamily::GoogleGenerativeAi
    );
    assert_eq!(
        registry
            .provider("amazon-bedrock")
            .expect("bedrock spec")
            .family,
        ModelProviderFamily::AwsBedrockConverse
    );
    assert_eq!(
        registry
            .provider("github-copilot")
            .expect("copilot spec")
            .family,
        ModelProviderFamily::GitHubCopilot
    );
    assert_eq!(
        registry.provider("gitlab").expect("gitlab spec").family,
        ModelProviderFamily::GitLabDuo
    );
    assert_eq!(
        registry.provider("openrouter").expect("openrouter spec").family,
        ModelProviderFamily::OpenAiCompatible
    );
}

#[test]
fn gateway_summary_lists_multiple_transport_families() {
    let gateway = ModelGateway::default();
    let summary = gateway.provider_summary();

    assert_eq!(summary.get("openai"), Some(&"openai-responses"));
    assert_eq!(summary.get("anthropic"), Some(&"anthropic-messages"));
    assert_eq!(summary.get("google-vertex"), Some(&"google-vertex"));
    assert_eq!(summary.get("amazon-bedrock"), Some(&"aws-bedrock-converse"));
}

#[test]
fn provider_override_updates_primary_provider_selection() {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        "openrouter".to_owned(),
        ProviderOverride {
            base_url: Some("https://openrouter.ai/api/v1".to_owned()),
            api_key: Some("demo-key".to_owned()),
            model_id: Some("openai/gpt-4.1-mini".to_owned()),
            headers: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
    );

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "openrouter".to_owned(),
        overrides,
    });

    assert_eq!(gateway.primary_provider_id(), "openrouter");
    assert!(
        gateway.primary_capabilities().tool_call_streaming,
        "openrouter should inherit generic openai-compatible tool streaming support",
    );
}

#[test]
fn special_providers_expose_explicit_auth_strategies() {
    let registry = ProviderRegistry::default();

    let openai = registry.provider("openai").expect("openai spec");
    assert_eq!(openai.auth_strategies.len(), 2);
    assert!(
        openai
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "chatgpt-oauth")
    );
    assert!(
        openai
            .auth_strategies
            .iter()
            .any(|strategy| strategy.env_keys.contains(&"OPENAI_API_KEY"))
    );

    let anthropic = registry.provider("anthropic").expect("anthropic spec");
    assert!(
        anthropic
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "claude-cli")
    );
    assert!(
        anthropic
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "setup-token")
    );

    let bedrock = registry
        .provider("amazon-bedrock")
        .expect("bedrock spec");
    assert!(
        bedrock
            .auth_strategies
            .iter()
            .any(|strategy| strategy.env_keys.contains(&"AWS_BEARER_TOKEN_BEDROCK"))
    );
    assert!(
        bedrock
            .auth_strategies
            .iter()
            .any(|strategy| strategy.env_keys.contains(&"AWS_PROFILE"))
    );

    let copilot = registry
        .provider("github-copilot")
        .expect("copilot spec");
    assert_eq!(copilot.auth_strategies.len(), 1);
    assert_eq!(copilot.auth_strategies[0].label, "device-code");

    let gitlab = registry.provider("gitlab").expect("gitlab spec");
    assert!(
        gitlab
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "oauth")
    );
    assert!(
        gitlab
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "personal-access-token")
    );

    let openai_codex = registry
        .provider("openai-codex")
        .expect("openai-codex spec");
    assert_eq!(openai_codex.auth_kind, liz_app_server::model::ProviderAuthKind::ApiKey);
    assert_eq!(openai_codex.auth_strategies.len(), 1);
    assert_eq!(openai_codex.auth_strategies[0].label, "api-key");

    let codex = registry.provider("codex").expect("codex spec");
    assert_eq!(codex.auth_kind, liz_app_server::model::ProviderAuthKind::ApiKey);
}

#[test]
fn gitlab_env_resolution_prefers_ai_gateway_and_keeps_instance_metadata() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("GITLAB_INSTANCE_URL", "https://gitlab.example.com");
    std::env::set_var("GITLAB_AI_GATEWAY_URL", "https://ai-gateway.example.com");

    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "gitlab".to_owned(),
        overrides: BTreeMap::new(),
    });
    let provider = gateway
        .resolved_primary_provider()
        .expect("gitlab provider should resolve");

    assert_eq!(provider.base_url.as_deref(), Some("https://ai-gateway.example.com"));
    assert_eq!(
        provider.metadata.get("gitlab.instance_url").map(String::as_str),
        Some("https://gitlab.example.com")
    );
    assert_eq!(
        provider.metadata.get("gitlab.ai_gateway_url").map(String::as_str),
        Some("https://ai-gateway.example.com")
    );

    std::env::remove_var("GITLAB_INSTANCE_URL");
    std::env::remove_var("GITLAB_AI_GATEWAY_URL");
}

#[test]
fn explicit_metadata_override_beats_process_env_for_bedrock_and_vertex() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("AWS_REGION", "us-east-1");
    std::env::set_var("GOOGLE_VERTEX_LOCATION", "us-central1");

    let mut overrides = BTreeMap::new();
    overrides.insert(
        "amazon-bedrock".to_owned(),
        ProviderOverride {
            base_url: None,
            api_key: None,
            model_id: None,
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(String::from("aws.region"), String::from("eu-west-1"))]),
        },
    );
    overrides.insert(
        "google-vertex".to_owned(),
        ProviderOverride {
            base_url: None,
            api_key: None,
            model_id: None,
            headers: BTreeMap::new(),
            metadata: BTreeMap::from([(
                String::from("google.location"),
                String::from("europe-west4"),
            )]),
        },
    );

    let bedrock = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "amazon-bedrock".to_owned(),
        overrides: overrides.clone(),
    })
    .resolved_primary_provider()
    .expect("bedrock provider should resolve");
    assert_eq!(
        bedrock.metadata.get("aws.region").map(String::as_str),
        Some("eu-west-1")
    );

    let vertex = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "google-vertex".to_owned(),
        overrides,
    })
    .resolved_primary_provider()
    .expect("vertex provider should resolve");
    assert_eq!(
        vertex.metadata.get("google.location").map(String::as_str),
        Some("europe-west4")
    );

    std::env::remove_var("AWS_REGION");
    std::env::remove_var("GOOGLE_VERTEX_LOCATION");
}

#[test]
fn openai_compatible_provider_without_builtin_base_url_still_runs() {
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "sap-ai-core".to_owned(),
        overrides: BTreeMap::new(),
    });
    let request = demo_request();
    let mut events = Vec::new();
    let summary = gateway
        .run_turn(request, |event| events.push(event))
        .expect("provider should still produce a request plan");

    assert!(summary.assistant_message.is_some());
    assert!(events.iter().any(|event| matches!(
        event,
        NormalizedTurnEvent::AssistantMessage { .. }
    )));
}

#[test]
fn gitlab_events_do_not_emit_patch_updates_when_capability_disables_patching() {
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "gitlab".to_owned(),
        overrides: BTreeMap::new(),
    });
    let request = demo_request();
    let mut events = Vec::new();
    gateway
        .run_turn(request, |event| events.push(event))
        .expect("gitlab provider should run");

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, NormalizedTurnEvent::ToolCallDelta { .. })),
        "gitlab should not emit patch deltas when patching is disabled",
    );
}

fn demo_request() -> ModelTurnRequest {
    ModelTurnRequest {
        thread: Thread {
            id: ThreadId::new("thread_test"),
            title: "Provider demo".to_owned(),
            status: ThreadStatus::Active,
            created_at: Timestamp::new("2026-04-13T20:00:00Z"),
            updated_at: Timestamp::new("2026-04-13T20:00:00Z"),
            active_goal: Some("Exercise provider runtime".to_owned()),
            active_summary: Some("Running provider demo".to_owned()),
            last_interruption: None,
            pending_commitments: Vec::new(),
            latest_turn_id: None,
            latest_checkpoint_id: None,
            parent_thread_id: None,
        },
        turn: Turn {
            id: TurnId::new("turn_test"),
            thread_id: ThreadId::new("thread_test"),
            kind: TurnKind::User,
            status: TurnStatus::Running,
            started_at: Timestamp::new("2026-04-13T20:00:01Z"),
            ended_at: None,
            goal: Some("Run a tool command".to_owned()),
            summary: None,
            checkpoint_before: None,
            checkpoint_after: None,
        },
        prompt: "Run a patch tool command for this task".to_owned(),
    }
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
