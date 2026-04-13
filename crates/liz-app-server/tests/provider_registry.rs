//! Provider-registry and provider-family plan coverage.

use liz_app_server::model::{
    ModelGateway, ModelGatewayConfig, ModelProviderFamily, ProviderOverride, ProviderRegistry,
};
use std::collections::BTreeMap;

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
