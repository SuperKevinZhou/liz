//! Builtin provider registry for the liz backend.

use crate::model::capabilities::ModelCapabilities;
use crate::model::family::ModelProviderFamily;
use crate::model::provider_spec::{ProviderAuthKind, ProviderAuthStrategy, ProviderSpec};
use std::collections::BTreeMap;

/// The builtin provider registry used by the liz backend.
#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    providers: BTreeMap<&'static str, ProviderSpec>,
}

impl ProviderRegistry {
    /// Returns the builtin provider spec for a given provider id.
    pub fn provider(&self, provider_id: &str) -> Option<&ProviderSpec> {
        self.providers.get(provider_id)
    }

    /// Returns all builtin provider specs keyed by id.
    pub fn providers(&self) -> &BTreeMap<&'static str, ProviderSpec> {
        &self.providers
    }

    /// Returns the sorted list of supported provider ids.
    pub fn supported_provider_ids(&self) -> Vec<&'static str> {
        self.providers.keys().copied().collect()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        let mut providers = BTreeMap::new();
        for spec in builtin_specs() {
            providers.insert(spec.id, spec);
        }
        Self { providers }
    }
}

fn builtin_specs() -> Vec<ProviderSpec> {
    vec![
        spec(
            "openai",
            "OpenAI",
            ModelProviderFamily::OpenAiResponses,
            ProviderAuthKind::Hybrid,
            Some("https://api.openai.com"),
            "gpt-5.4",
            &["OPENAI_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_streaming().with_prompt_caching(true),
            &["Primary OpenAI Responses-style provider.", "Supports ChatGPT OAuth or API key."],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::OAuth,
                "chatgpt-oauth",
                &[],
                &[],
                &["Supports ChatGPT OAuth and direct API-key authentication."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "api-key",
                &["OPENAI_API_KEY"],
                &["models.providers.openai.baseUrl"],
                &["Direct API-key auth with optional provider base URL override."],
            ),
        ]),
        spec(
            "anthropic",
            "Anthropic",
            ModelProviderFamily::AnthropicMessages,
            ProviderAuthKind::Hybrid,
            Some("https://api.anthropic.com"),
            "claude-sonnet-4-6",
            &["ANTHROPIC_API_KEY"],
            &[],
            &[(
                "anthropic-beta",
                "interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14",
            )],
            ModelCapabilities::anthropic_messages(),
            &["Anthropic Messages family.", "Enables beta headers for streaming and tool use."],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::SetupToken,
                "setup-token",
                &[],
                &[],
                &["Supports a runtime setup-token flow."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "api-key",
                &["ANTHROPIC_API_KEY"],
                &["models.providers.anthropic.baseUrl"],
                &["Direct Anthropic API key path."],
            ),
        ]),
        spec(
            "google",
            "Google",
            ModelProviderFamily::GoogleGenerativeAi,
            ProviderAuthKind::ApiKey,
            Some("https://generativelanguage.googleapis.com"),
            "gemini-3.1-pro",
            &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
            &[],
            &[],
            ModelCapabilities::google_family(),
            &["Google Generative AI direct provider."],
        ),
        spec(
            "google-vertex",
            "Google Vertex AI",
            ModelProviderFamily::GoogleVertex,
            ProviderAuthKind::GoogleApplicationDefault,
            None,
            "gemini-3.1-pro",
            &[],
            &[
                "GOOGLE_CLOUD_PROJECT",
                "GCP_PROJECT",
                "GCLOUD_PROJECT",
                "GOOGLE_VERTEX_LOCATION",
            ],
            &[],
            ModelCapabilities::google_family()
                .with_server_side_conversation_state(false)
                .with_reasoning_token_accounting(false),
            &["Vertex AI provider resolved through Google ADC and project/location env."],
        ),
        spec(
            "amazon-bedrock",
            "Amazon Bedrock",
            ModelProviderFamily::AwsBedrockConverse,
            ProviderAuthKind::AwsCredentialChain,
            None,
            "anthropic.claude-sonnet-4-6-v1:0",
            &["AWS_BEARER_TOKEN_BEDROCK"],
            &["AWS_REGION", "AWS_PROFILE"],
            &[],
            ModelCapabilities::bedrock_converse(),
            &[
                "Uses Bedrock Converse/ConverseStream semantics.",
                "Accepts Bedrock runtime endpoints, foundation model IDs, and inference profiles.",
            ],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "bedrock-bearer-token",
                &["AWS_BEARER_TOKEN_BEDROCK"],
                &["provider.amazon-bedrock.options.region", "provider.amazon-bedrock.options.endpoint"],
                &["Bearer-token auth takes precedence over the AWS credential chain."],
            ),
            auth_strategy(
                ProviderAuthKind::AwsCredentialChain,
                "aws-credential-chain",
                &[
                    "AWS_ACCESS_KEY_ID",
                    "AWS_SECRET_ACCESS_KEY",
                    "AWS_PROFILE",
                    "AWS_WEB_IDENTITY_TOKEN_FILE",
                    "AWS_ROLE_ARN",
                ],
                &["provider.amazon-bedrock.options.profile", "provider.amazon-bedrock.options.region"],
                &["Falls back to shared credentials, profiles, web identity, or instance metadata."],
            ),
        ]),
        spec(
            "amazon-bedrock-mantle",
            "Amazon Bedrock Mantle",
            ModelProviderFamily::AwsBedrockConverse,
            ProviderAuthKind::AwsCredentialChain,
            None,
            "anthropic.claude-sonnet-4-6-v1:0",
            &["AWS_BEARER_TOKEN_BEDROCK"],
            &["AWS_REGION", "AWS_PROFILE"],
            &[],
            ModelCapabilities::bedrock_converse(),
            &["Bedrock bridge-style provider variant."],
        ),
        spec(
            "google-vertex-anthropic",
            "Google Vertex Anthropic",
            ModelProviderFamily::GoogleVertexAnthropic,
            ProviderAuthKind::GoogleApplicationDefault,
            None,
            "claude-sonnet-4-6",
            &[],
            &[
                "GOOGLE_CLOUD_PROJECT",
                "GCP_PROJECT",
                "GCLOUD_PROJECT",
                "GOOGLE_VERTEX_LOCATION",
            ],
            &[],
            ModelCapabilities::anthropic_messages().with_max_context_window(1_000_000),
            &["Anthropic models hosted on Vertex AI."],
        ),
        spec(
            "anthropic-vertex",
            "Anthropic Vertex",
            ModelProviderFamily::GoogleVertexAnthropic,
            ProviderAuthKind::GoogleApplicationDefault,
            None,
            "claude-sonnet-4-6",
            &[],
            &[
                "GOOGLE_CLOUD_PROJECT",
                "GCP_PROJECT",
                "GCLOUD_PROJECT",
                "GOOGLE_VERTEX_LOCATION",
            ],
            &[],
            ModelCapabilities::anthropic_messages().with_max_context_window(1_000_000),
            &["Dedicated Anthropic-on-Vertex provider variant."],
        ),
        spec(
            "github-copilot",
            "GitHub Copilot",
            ModelProviderFamily::GitHubCopilot,
            ProviderAuthKind::DeviceCode,
            Some("https://api.githubcopilot.com"),
            "gpt-4o",
            &["GITHUB_COPILOT_TOKEN"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_strict_tool_schema(true)
                .with_server_side_conversation_state(true),
            &["Device auth and model discovery are provider-owned.", "GPT-5 Copilot models favor the Responses path."],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::DeviceCode,
            "device-code",
            &["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"],
            &["provider.github-copilot.options.enterpriseUrl"],
            &["Built-in plugin exchanges a GitHub token for Copilot runtime auth."],
        )]),
        spec(
            "gitlab",
            "GitLab Duo",
            ModelProviderFamily::GitLabDuo,
            ProviderAuthKind::Hybrid,
            Some("https://gitlab.com"),
            "duo-chat-sonnet-4-5",
            &["GITLAB_TOKEN"],
            &["GITLAB_INSTANCE_URL", "GITLAB_AI_GATEWAY_URL", "GITLAB_OAUTH_CLIENT_ID"],
            &[("anthropic-beta", "context-1m-2025-08-07")],
            ModelCapabilities::anthropic_messages()
                .with_server_side_conversation_state(true)
                .with_max_context_window(1_000_000),
            &["GitLab Duo Agent Platform provider.", "Workflow-model discovery is provider-owned."],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::OAuth,
                "oauth",
                &[],
                &["GITLAB_INSTANCE_URL", "GITLAB_OAUTH_CLIENT_ID", "provider.gitlab.options.instanceUrl"],
                &["Supports OAuth on GitLab.com and self-hosted GitLab."],
            ),
            auth_strategy(
                ProviderAuthKind::PersonalAccessToken,
                "personal-access-token",
                &["GITLAB_TOKEN"],
                &["GITLAB_INSTANCE_URL", "GITLAB_AI_GATEWAY_URL"],
                &["Supports personal access token authentication."],
            ),
        ]),
        spec(
            "openai-codex",
            "OpenAI Codex",
            ModelProviderFamily::OpenAiResponses,
            ProviderAuthKind::OAuth,
            Some("https://chatgpt.com/backend-api"),
            "gpt-5.4",
            &["OPENAI_CODEX_ACCESS_TOKEN"],
            &[],
            &[],
            ModelCapabilities::openai_streaming()
                .with_prompt_caching(true)
                .with_server_side_conversation_state(true),
            &[
                "Native Codex OAuth-backed Responses route.",
                "Uses ChatGPT/Codex OAuth tokens against chatgpt.com/backend-api/codex/responses.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::OAuth,
            "chatgpt-oauth",
            &["OPENAI_CODEX_ACCESS_TOKEN", "OPENAI_CODEX_REFRESH_TOKEN"],
            &[
                "provider.openai-codex.oauth.refreshToken",
                "provider.openai-codex.oauth.accountId",
                "provider.openai-codex.oauth.expiresAtMs",
            ],
            &["Native ChatGPT/Codex OAuth path with refresh-token support."],
        )]),
        spec(
            "xai",
            "xAI",
            ModelProviderFamily::OpenAiResponses,
            ProviderAuthKind::ApiKey,
            Some("https://api.x.ai"),
            "grok-4",
            &["XAI_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_streaming(),
            &["Uses Responses-style request handling."],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["XAI_API_KEY"],
            &["models.providers.xai.baseUrl"],
            &["Direct xAI API key path."],
        )]),
        openai_compatible_spec("openrouter", "OpenRouter", "openai/gpt-4.1-mini"),
        openai_compatible_spec("ollama", "Ollama", "llama3.1:8b"),
        openai_compatible_spec("vllm", "vLLM", "meta-llama/Llama-3.1-70B-Instruct"),
        openai_compatible_spec("sglang", "SGLang", "meta-llama/Llama-3.1-70B-Instruct"),
        openai_compatible_spec("opencode", "OpenCode Zen", "claude-opus-4-6"),
        openai_compatible_spec("opencode-go", "OpenCode Go", "kimi-k2.5"),
        openai_compatible_spec("qwen", "Qwen", "qwen-max"),
        openai_compatible_spec("zai", "Z.AI", "glm-5"),
        openai_compatible_spec(
            "vercel-ai-gateway",
            "Vercel AI Gateway",
            "anthropic/claude-opus-4.6",
        ),
        openai_compatible_spec(
            "cloudflare-ai-gateway",
            "Cloudflare AI Gateway",
            "anthropic/claude-sonnet-4-6",
        ),
        openai_compatible_spec("cloudflare-workers-ai", "Cloudflare Workers AI", "@cf/meta/llama-3.1-70b-instruct"),
        openai_compatible_spec("sap-ai-core", "SAP AI Core", "anthropic/claude-sonnet-4-6"),
        openai_compatible_spec("deepseek", "DeepSeek", "deepseek-chat"),
        openai_compatible_spec("mistral", "Mistral", "mistral-large-latest"),
        openai_compatible_spec("moonshot", "Moonshot", "moonshot-v1-128k"),
        openai_compatible_spec("moonshotai", "Moonshot AI", "moonshot-v1-128k"),
        openai_compatible_spec("minimax", "MiniMax", "MiniMax-M2.7"),
        openai_compatible_spec("litellm", "LiteLLM", "anthropic/claude-opus-4.6"),
        openai_compatible_spec("huggingface", "Hugging Face", "Qwen/Qwen3-Coder-480B-A35B-Instruct"),
        openai_compatible_spec("together", "Together AI", "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8"),
        openai_compatible_spec("togetherai", "Together AI", "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8"),
        openai_compatible_spec("venice", "Venice AI", "venice-uncensored"),
        openai_compatible_spec("qianfan", "Qianfan", "ernie-4.5-8k"),
        openai_compatible_spec("synthetic", "Synthetic", "Synthetic-1"),
        openai_compatible_spec("xiaomi", "Xiaomi", "mimo-v2-flash"),
        openai_compatible_spec("kimi", "Kimi", "kimi-k2.5"),
        openai_compatible_spec("azure", "Azure OpenAI", "gpt-4.1"),
        openai_compatible_spec("azure-cognitive-services", "Azure Cognitive Services", "gpt-4.1"),
        openai_compatible_spec("copilot-proxy", "Copilot Proxy", "gpt-5-mini"),
        openai_compatible_spec("microsoft-foundry", "Microsoft Foundry", "gpt-4.1"),
        openai_compatible_spec("302ai", "302.AI", "gpt-4.1-mini"),
        openai_compatible_spec("arcee", "Arcee", "arcee-ai/coder-large"),
        openai_compatible_spec("baseten", "Baseten", "deepseek-ai/DeepSeek-R1"),
        openai_compatible_spec("byteplus", "BytePlus", "doubao-1.5-pro"),
        openai_compatible_spec("byteplus-plan", "BytePlus Plan", "doubao-1.5-thinking-pro"),
        openai_compatible_spec("cerebras", "Cerebras", "llama-4-scout"),
        openai_compatible_spec("chutes", "Chutes", "deepseek-ai/DeepSeek-V3"),
        openai_compatible_spec("cohere", "Cohere", "command-a"),
        openai_compatible_spec("cortecs", "Cortecs", "moonshotai/kimi-k2-instruct"),
        openai_compatible_spec("deepinfra", "DeepInfra", "meta-llama/Llama-3.1-70B-Instruct"),
        openai_compatible_spec("firmware", "Firmware", "gpt-4.1-mini"),
        openai_compatible_spec("fireworks", "Fireworks AI", "accounts/fireworks/models/llama-v3p1-70b-instruct"),
        openai_compatible_spec("fireworks-ai", "Fireworks AI", "accounts/fireworks/models/llama-v3p1-70b-instruct"),
        openai_compatible_spec("groq", "Groq", "llama-3.3-70b-versatile"),
        openai_compatible_spec("helicone", "Helicone", "gpt-4.1-mini"),
        openai_compatible_spec("io-net", "IO.NET", "meta-llama/Llama-3.3-70B-Instruct"),
        openai_compatible_spec("kilo", "Kilo", "gpt-4.1-mini"),
        openai_compatible_spec("kilocode", "KiloCode", "kilocode/kilo-agent"),
        openai_compatible_spec("llama.cpp", "llama.cpp", "meta-llama/Llama-3.1-8B-Instruct"),
        openai_compatible_spec("lmstudio", "LM Studio", "google/gemma-3n-e4b"),
        openai_compatible_spec("minimax-portal", "MiniMax Portal", "MiniMax-M2.7"),
        openai_compatible_spec("nebius-token-factory", "Nebius Token Factory", "moonshotai/kimi-k2-instruct"),
        openai_compatible_spec("nebius", "Nebius Token Factory", "moonshotai/kimi-k2-instruct"),
        openai_compatible_spec("nvidia", "NVIDIA", "meta/llama-3.1-70b-instruct"),
        openai_compatible_spec("ollama-cloud", "Ollama Cloud", "gpt-oss:20b-cloud"),
        openai_compatible_spec("ovhcloud-ai-endpoints", "OVHcloud AI Endpoints", "gpt-oss-120b"),
        openai_compatible_spec("ovhcloud", "OVHcloud AI Endpoints", "gpt-oss-120b"),
        openai_compatible_spec("poe", "Poe", "anthropic/claude-sonnet-4-6"),
        openai_compatible_spec("scaleway", "Scaleway", "devstral-2-123b-instruct-2512"),
        openai_compatible_spec("stackit", "STACKIT", "Llama-3.3-70B-Instruct"),
        openai_compatible_spec("stepfun", "StepFun", "step-2"),
        openai_compatible_spec("stepfun-plan", "StepFun Plan", "step-2-thinking"),
        openai_compatible_spec("volcengine", "Volcengine", "doubao-1.5-pro"),
        openai_compatible_spec("volcengine-plan", "Volcengine Plan", "doubao-1.5-thinking-pro"),
        openai_compatible_spec("vercel", "Vercel AI Gateway", "anthropic/claude-sonnet-4"),
        openai_compatible_spec("zenmux", "ZenMux", "anthropic/claude-sonnet-4-6"),
    ]
}

fn spec(
    id: &'static str,
    display_name: &'static str,
    family: ModelProviderFamily,
    auth_kind: ProviderAuthKind,
    default_base_url: Option<&'static str>,
    default_model: &'static str,
    api_key_envs: &'static [&'static str],
    required_envs: &'static [&'static str],
    default_headers: &'static [(&'static str, &'static str)],
    capabilities: ModelCapabilities,
    notes: &'static [&'static str],
) -> ProviderSpec {
    ProviderSpec {
        id,
        display_name,
        family,
        auth_kind,
        auth_strategies: vec![auth_strategy(auth_kind, auth_kind.label(), api_key_envs, required_envs, &[])],
        default_base_url,
        default_model,
        api_key_envs,
        required_envs,
        default_headers,
        capabilities,
        notes,
    }
}

fn openai_compatible_spec(
    id: &'static str,
    display_name: &'static str,
    default_model: &'static str,
) -> ProviderSpec {
    let default_base_url = match id {
        "openrouter" => Some("https://openrouter.ai/api/v1"),
        "ollama" => Some("http://localhost:11434/v1"),
        "ollama-cloud" => Some("https://ollama.com/api"),
        "lmstudio" => Some("http://127.0.0.1:1234/v1"),
        "llama.cpp" => Some("http://127.0.0.1:8080/v1"),
        "vllm" => Some("http://127.0.0.1:8000/v1"),
        "sglang" => Some("http://127.0.0.1:30000/v1"),
        "vercel-ai-gateway" | "vercel" => Some("https://ai-gateway.vercel.sh/v1"),
        "302ai" => Some("https://api.302.ai/v1"),
        "cortecs" => Some("https://api.cortecs.ai/v1"),
        "deepseek" => Some("https://api.deepseek.com"),
        "helicone" => Some("https://ai-gateway.helicone.ai/v1"),
        "io-net" => Some("https://api.intelligence.io.solutions/api/v1"),
        "minimax" | "minimax-portal" => Some("https://api.minimax.io/anthropic/v1"),
        "moonshot" | "moonshotai" => Some("https://api.moonshot.ai/v1"),
        "nebius" | "nebius-token-factory" => Some("https://api.tokenfactory.nebius.com/v1"),
        "ovhcloud" | "ovhcloud-ai-endpoints" => Some("https://oai.endpoints.kepler.ai.cloud.ovh.net/v1"),
        "scaleway" => Some("https://api.scaleway.ai/v1"),
        "stackit" => Some("https://api.openai-compat.model-serving.eu01.onstackit.cloud/v1"),
        "zai" => Some("https://api.z.ai/api/paas/v4"),
        _ => None,
    };

    let api_key_envs = match id {
        "openrouter" => &["OPENROUTER_API_KEY"][..],
        "ollama" | "llama.cpp" | "lmstudio" | "vllm" | "sglang" => &[][..],
        "opencode" | "opencode-go" => &["OPENCODE_API_KEY"][..],
        "github-copilot" => &["GITHUB_COPILOT_TOKEN"][..],
        "gitlab" => &["GITLAB_TOKEN"][..],
        "sap-ai-core" => &["AICORE_SERVICE_KEY"][..],
        "cloudflare-ai-gateway" => &["CLOUDFLARE_API_TOKEN", "CLOUDFLARE_AI_GATEWAY_API_KEY"][..],
        "cloudflare-workers-ai" => &["CLOUDFLARE_API_KEY"][..],
        "azure" => &["AZURE_OPENAI_API_KEY", "AZURE_API_KEY"][..],
        "azure-cognitive-services" => &["AZURE_API_KEY"][..],
        "xai" => &["XAI_API_KEY"][..],
        "mistral" => &["MISTRAL_API_KEY"][..],
        "moonshot" | "moonshotai" => &["MOONSHOT_API_KEY"][..],
        "minimax" | "minimax-portal" => &["MINIMAX_API_KEY"][..],
        "together" | "togetherai" => &["TOGETHER_API_KEY"][..],
        "venice" => &["VENICE_API_KEY"][..],
        "qianfan" => &["QIANFAN_API_KEY"][..],
        "synthetic" => &["SYNTHETIC_API_KEY"][..],
        "xiaomi" => &["XIAOMI_API_KEY"][..],
        "kimi" => &["KIMI_API_KEY"][..],
        "huggingface" => &["HF_TOKEN", "HUGGINGFACE_HUB_TOKEN"][..],
        "litellm" => &["LITELLM_API_KEY"][..],
        "deepseek" => &["DEEPSEEK_API_KEY"][..],
        "cerebras" => &["CEREBRAS_API_KEY"][..],
        "cohere" => &["COHERE_API_KEY"][..],
        "groq" => &["GROQ_API_KEY"][..],
        "zenmux" => &["ZENMUX_API_KEY"][..],
        "302ai" => &["302AI_API_KEY"][..],
        "baseten" => &["BASETEN_API_KEY"][..],
        "cortecs" => &["CORTECS_API_KEY"][..],
        "firmware" => &["FIRMWARE_API_KEY"][..],
        "fireworks" | "fireworks-ai" => &["FIREWORKS_API_KEY"][..],
        "io-net" => &["IOINTELLIGENCE_API_KEY"][..],
        "nebius" | "nebius-token-factory" => &["NEBIUS_API_KEY"][..],
        "ollama-cloud" => &["OLLAMA_API_KEY"][..],
        "ovhcloud" | "ovhcloud-ai-endpoints" => &["OVHCLOUD_API_KEY"][..],
        "stackit" => &["STACKIT_API_KEY"][..],
        "zai" => &["ZHIPU_API_KEY"][..],
        "vercel" | "vercel-ai-gateway" => &["AI_GATEWAY_API_KEY"][..],
        _ => &[],
    };

    let required_envs = match id {
        "azure" => &["AZURE_RESOURCE_NAME"][..],
        "azure-cognitive-services" => &["AZURE_COGNITIVE_SERVICES_RESOURCE_NAME"][..],
        "cloudflare-ai-gateway" => &["CLOUDFLARE_ACCOUNT_ID", "CLOUDFLARE_GATEWAY_ID"][..],
        "cloudflare-workers-ai" => &["CLOUDFLARE_ACCOUNT_ID"][..],
        _ => &[],
    };

    let default_headers = match id {
        "openrouter" => &[("HTTP-Referer", "https://liz.local/"), ("X-Title", "liz")][..],
        "vercel-ai-gateway" => &[("http-referer", "https://liz.local/"), ("x-title", "liz")][..],
        "zenmux" | "kilo" => &[("HTTP-Referer", "https://liz.local/"), ("X-Title", "liz")][..],
        _ => &[][..],
    };

    let auth_kind = if matches!(id, "ollama" | "llama.cpp" | "lmstudio" | "vllm" | "sglang") {
        ProviderAuthKind::Local
    } else if id == "sap-ai-core" {
        ProviderAuthKind::ServiceKey
    } else if id == "github-copilot" {
        ProviderAuthKind::DeviceCode
    } else if id == "gitlab" {
        ProviderAuthKind::Hybrid
    } else {
        ProviderAuthKind::ApiKey
    };

    spec(
        id,
        display_name,
        ModelProviderFamily::OpenAiCompatible,
        auth_kind,
        default_base_url,
        default_model,
        api_key_envs,
        required_envs,
        default_headers,
        ModelCapabilities::openai_compatible()
            .with_tool_call_streaming(true)
            .with_image_input(true)
            .with_max_context_window(200_000),
        &["OpenAI-compatible or gateway-style provider resolved through the generic adapter."],
    )
}

fn auth_strategy(
    kind: ProviderAuthKind,
    label: &'static str,
    env_keys: &'static [&'static str],
    config_keys: &'static [&'static str],
    notes: &'static [&'static str],
) -> ProviderAuthStrategy {
    ProviderAuthStrategy {
        kind,
        label,
        env_keys,
        config_keys,
        notes,
    }
}
