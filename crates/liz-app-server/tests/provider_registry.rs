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
        "amazon-bedrock-mantle",
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
            .provider("amazon-bedrock-mantle")
            .expect("bedrock mantle spec")
            .family,
        ModelProviderFamily::OpenAiCompatible
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
    assert_eq!(openai.auth_kind, liz_app_server::model::ProviderAuthKind::ApiKey);
    assert_eq!(openai.auth_strategies.len(), 1);
    assert_eq!(openai.auth_strategies[0].label, "api-key");
    assert!(
        openai
            .auth_strategies
            .iter()
            .any(|strategy| strategy.env_keys.contains(&"OPENAI_API_KEY"))
    );

    let anthropic = registry.provider("anthropic").expect("anthropic spec");
    assert_eq!(
        anthropic.auth_kind,
        liz_app_server::model::ProviderAuthKind::ApiKey
    );
    assert_eq!(anthropic.auth_strategies.len(), 1);
    assert_eq!(anthropic.auth_strategies[0].label, "api-key");

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

    let mantle = registry
        .provider("amazon-bedrock-mantle")
        .expect("bedrock mantle spec");
    assert_eq!(mantle.auth_kind, liz_app_server::model::ProviderAuthKind::AwsCredentialChain);
    assert!(
        mantle
            .auth_strategies
            .iter()
            .any(|strategy| strategy.env_keys.contains(&"AWS_BEARER_TOKEN_BEDROCK"))
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
    assert_eq!(openai_codex.auth_kind, liz_app_server::model::ProviderAuthKind::OAuth);
    assert_eq!(openai_codex.auth_strategies.len(), 1);
    assert_eq!(openai_codex.auth_strategies[0].label, "chatgpt-oauth");

    let sap_ai_core = registry.provider("sap-ai-core").expect("sap-ai-core spec");
    assert_eq!(
        sap_ai_core.auth_kind,
        liz_app_server::model::ProviderAuthKind::ServiceKey
    );
    assert!(
        sap_ai_core
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "service-key")
    );

    let qwen = registry.provider("qwen").expect("qwen spec");
    assert_eq!(qwen.auth_kind, liz_app_server::model::ProviderAuthKind::ApiKey);
    assert!(
        qwen
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "coding-plan-global")
    );
    assert!(
        qwen
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "standard-global")
    );

    let zai = registry.provider("zai").expect("zai spec");
    assert_eq!(zai.auth_kind, liz_app_server::model::ProviderAuthKind::ApiKey);
    assert!(
        zai
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "coding-plan-global")
    );
    assert!(
        zai
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "cn")
    );

    let minimax = registry.provider("minimax").expect("minimax spec");
    assert_eq!(
        minimax.auth_kind,
        liz_app_server::model::ProviderAuthKind::ApiKey
    );
    assert!(
        minimax
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "api-global")
    );

    let minimax_portal = registry
        .provider("minimax-portal")
        .expect("minimax portal spec");
    assert_eq!(
        minimax_portal.auth_kind,
        liz_app_server::model::ProviderAuthKind::OAuth
    );
    assert!(
        minimax_portal
            .auth_strategies
            .iter()
            .any(|strategy| strategy.label == "oauth-global")
    );

    let kimi = registry.provider("kimi").expect("kimi spec");
    assert_eq!(kimi.family, ModelProviderFamily::AnthropicMessages);
    assert_eq!(kimi.default_base_url, Some("https://api.kimi.com/coding"));

    let byteplus_plan = registry
        .provider("byteplus-plan")
        .expect("byteplus-plan spec");
    assert_eq!(
        byteplus_plan.default_base_url,
        Some("https://ark.ap-southeast.bytepluses.com/api/coding/v3")
    );

    let volcengine_plan = registry
        .provider("volcengine-plan")
        .expect("volcengine-plan spec");
    assert_eq!(
        volcengine_plan.default_base_url,
        Some("https://ark.cn-beijing.volces.com/api/coding/v3")
    );

    let stepfun_plan = registry
        .provider("stepfun-plan")
        .expect("stepfun-plan spec");
    assert_eq!(
        stepfun_plan.default_base_url,
        Some("https://api.stepfun.ai/step_plan/v1")
    );
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
fn mantle_env_resolution_derives_region_scoped_endpoint() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("AWS_REGION", "us-west-2");

    let provider = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "amazon-bedrock-mantle".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("bedrock mantle provider should resolve");

    assert_eq!(
        provider.base_url.as_deref(),
        Some("https://bedrock-mantle.us-west-2.api.aws/v1")
    );
    assert_eq!(
        provider.metadata.get("aws.region").map(String::as_str),
        Some("us-west-2")
    );

    std::env::remove_var("AWS_REGION");
}

#[test]
fn qwen_env_resolution_supports_standard_and_coding_plan_hosts() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("QWEN_ENDPOINT", "standard-global");
    std::env::set_var("QWEN_API_KEY", "qwen-key");

    let standard = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "qwen".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("qwen provider should resolve");

    assert_eq!(
        standard.base_url.as_deref(),
        Some("https://dashscope-intl.aliyuncs.com/compatible-mode/v1")
    );
    assert_eq!(standard.model_id, "qwen3.5-plus");

    std::env::set_var("QWEN_ENDPOINT", "coding-cn");
    let coding = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "qwen".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("qwen coding provider should resolve");

    assert_eq!(
        coding.base_url.as_deref(),
        Some("https://coding.dashscope.aliyuncs.com/v1")
    );
    assert_eq!(
        coding.metadata.get("qwen.endpoint").map(String::as_str),
        Some("coding-cn")
    );

    std::env::remove_var("QWEN_ENDPOINT");
    std::env::remove_var("QWEN_API_KEY");
}

#[test]
fn zai_env_resolution_supports_forced_coding_plan_endpoint() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("ZAI_API_KEY", "zai-key");
    std::env::set_var("ZAI_ENDPOINT", "coding-cn");

    let provider = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "zai".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("zai provider should resolve");

    assert_eq!(
        provider.base_url.as_deref(),
        Some("https://open.bigmodel.cn/api/coding/paas/v4")
    );
    assert_eq!(provider.model_id, "glm-5.1");
    assert_eq!(
        provider.metadata.get("zai.endpoint").map(String::as_str),
        Some("coding-cn")
    );

    std::env::remove_var("ZAI_API_KEY");
    std::env::remove_var("ZAI_ENDPOINT");
}

#[test]
fn minimax_env_resolution_supports_global_and_cn_routes() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("MINIMAX_API_KEY", "minimax-key");
    std::env::set_var("MINIMAX_REGION", "cn");

    let minimax = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "minimax".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("minimax provider should resolve");

    assert_eq!(
        minimax.base_url.as_deref(),
        Some("https://api.minimaxi.com/anthropic")
    );
    assert_eq!(
        minimax.metadata.get("minimax.region").map(String::as_str),
        Some("cn")
    );

    std::env::set_var("MINIMAX_OAUTH_TOKEN", "portal-token");
    let minimax_portal = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "minimax-portal".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("minimax portal provider should resolve");

    assert_eq!(
        minimax_portal.base_url.as_deref(),
        Some("https://api.minimaxi.com/anthropic")
    );
    assert_eq!(
        minimax_portal.metadata.get("minimax.region").map(String::as_str),
        Some("cn")
    );

    std::env::remove_var("MINIMAX_API_KEY");
    std::env::remove_var("MINIMAX_OAUTH_TOKEN");
    std::env::remove_var("MINIMAX_REGION");
}

#[test]
fn stepfun_env_resolution_supports_plan_region_selection() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("STEPFUN_API_KEY", "stepfun-key");
    std::env::set_var("STEPFUN_REGION", "cn");

    let provider = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "stepfun-plan".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("stepfun-plan provider should resolve");

    assert_eq!(
        provider.base_url.as_deref(),
        Some("https://api.stepfun.com/step_plan/v1")
    );
    assert_eq!(
        provider.metadata.get("stepfun.region").map(String::as_str),
        Some("cn")
    );

    std::env::remove_var("STEPFUN_API_KEY");
    std::env::remove_var("STEPFUN_REGION");
}

#[test]
fn sap_ai_core_env_resolution_parses_service_key_and_builds_deployment_route() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var(
        "AICORE_SERVICE_KEY",
        r#"{"clientid":"sap-client","clientsecret":"sap-secret","url":"https://sap.example.com","serviceurls":{"AI_API_URL":"https://sap-api.example.com"}}"#,
    );
    std::env::set_var("AICORE_DEPLOYMENT_ID", "deployment-prod");
    std::env::set_var("AICORE_RESOURCE_GROUP", "rg-prod");

    let provider = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "sap-ai-core".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("sap-ai-core provider should resolve");

    assert_eq!(
        provider.base_url.as_deref(),
        Some("https://sap-api.example.com/v2/inference/deployments/deployment-prod")
    );
    assert_eq!(provider.model_id, "deployment-prod");
    assert_eq!(
        provider
            .metadata
            .get("sap_ai_core.oauth_base_url")
            .map(String::as_str),
        Some("https://sap.example.com")
    );
    assert_eq!(
        provider
            .metadata
            .get("sap_ai_core.resource_group")
            .map(String::as_str),
        Some("rg-prod")
    );

    std::env::remove_var("AICORE_SERVICE_KEY");
    std::env::remove_var("AICORE_DEPLOYMENT_ID");
    std::env::remove_var("AICORE_RESOURCE_GROUP");
}

#[test]
fn azure_env_resolution_uses_v1_base_urls_and_deployment_names() {
    let _guard = env_lock().lock().expect("env lock");
    std::env::set_var("AZURE_RESOURCE_NAME", "azure-openai-resource");
    std::env::set_var("AZURE_OPENAI_DEPLOYMENT", "gpt-4.1-prod");
    std::env::set_var("AZURE_COGNITIVE_SERVICES_RESOURCE_NAME", "azure-cog-resource");
    std::env::set_var("AZURE_COGNITIVE_SERVICES_DEPLOYMENT", "gpt-4.1-cog");

    let azure = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "azure".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("azure provider should resolve");
    assert_eq!(
        azure.base_url.as_deref(),
        Some("https://azure-openai-resource.openai.azure.com/openai/v1")
    );
    assert_eq!(azure.model_id, "gpt-4.1-prod");

    let cognitive = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "azure-cognitive-services".to_owned(),
        overrides: BTreeMap::new(),
    })
    .resolved_primary_provider()
    .expect("azure cognitive provider should resolve");
    assert_eq!(
        cognitive.base_url.as_deref(),
        Some("https://azure-cog-resource.cognitiveservices.azure.com/openai/v1")
    );
    assert_eq!(cognitive.model_id, "gpt-4.1-cog");

    std::env::remove_var("AZURE_RESOURCE_NAME");
    std::env::remove_var("AZURE_OPENAI_DEPLOYMENT");
    std::env::remove_var("AZURE_COGNITIVE_SERVICES_RESOURCE_NAME");
    std::env::remove_var("AZURE_COGNITIVE_SERVICES_DEPLOYMENT");
}

#[test]
fn openai_compatible_provider_without_builtin_base_url_still_runs() {
    let gateway = ModelGateway::from_config(ModelGatewayConfig {
        primary_provider: "copilot-proxy".to_owned(),
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
