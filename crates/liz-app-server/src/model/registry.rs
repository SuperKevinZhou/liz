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
            ProviderAuthKind::ApiKey,
            Some("https://api.openai.com"),
            "gpt-5.4",
            &["OPENAI_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_streaming().with_prompt_caching(true),
            &["Primary OpenAI Responses-style provider.", "Uses direct API-key authentication."],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["OPENAI_API_KEY"],
            &["models.providers.openai.baseUrl"],
            &["Direct API-key auth with optional provider base URL override."],
        )]),
        spec(
            "anthropic",
            "Anthropic",
            ModelProviderFamily::AnthropicMessages,
            ProviderAuthKind::ApiKey,
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
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["ANTHROPIC_API_KEY"],
            &["models.providers.anthropic.baseUrl"],
            &["Direct Anthropic API key path."],
        )]),
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
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::AwsCredentialChain,
            None,
            "gpt-oss-120b",
            &["AWS_BEARER_TOKEN_BEDROCK"],
            &["AWS_REGION", "AWS_PROFILE"],
            &[],
            ModelCapabilities::openai_compatible(),
            &[
                "Uses the Bedrock Mantle OpenAI-compatible endpoint.",
                "Accepts explicit Bedrock bearer tokens or IAM-derived bearer tokens.",
            ],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "bedrock-bearer-token",
                &["AWS_BEARER_TOKEN_BEDROCK"],
                &[
                    "provider.amazon-bedrock-mantle.options.region",
                    "provider.amazon-bedrock-mantle.options.endpoint",
                ],
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
                &[
                    "provider.amazon-bedrock-mantle.options.profile",
                    "provider.amazon-bedrock-mantle.options.region",
                ],
                &["Falls back to shared credentials, profiles, web identity, or instance metadata."],
            ),
        ]),
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
        spec(
            "opencode",
            "OpenCode Zen",
            ModelProviderFamily::OpenAiResponses,
            ProviderAuthKind::ApiKey,
            Some("https://opencode.ai/zen/v1"),
            "gpt-5.4",
            &["OPENCODE_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_streaming().with_prompt_caching(true),
            &[
                "Uses the OpenCode Zen GPT-family Responses endpoint.",
                "Routes default GPT models through https://opencode.ai/zen/v1/responses.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["OPENCODE_API_KEY"],
            &["provider.opencode.baseUrl"],
            &["Sends the OpenCode Zen API key as a bearer token."],
        )]),
        spec(
            "opencode-go",
            "OpenCode Go",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://opencode.ai/zen/go/v1"),
            "kimi-k2.5",
            &["OPENCODE_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(200_000),
            &[
                "Uses the OpenCode Go OpenAI-compatible chat completions endpoint.",
                "Routes the default Go model lane through https://opencode.ai/zen/go/v1/chat/completions.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["OPENCODE_API_KEY"],
            &["provider.opencode-go.baseUrl"],
            &["Sends the OpenCode Go API key as a bearer token."],
        )]),
        spec(
            "qwen",
            "Qwen",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://coding-intl.dashscope.aliyuncs.com/v1"),
            "qwen3.5-plus",
            &["QWEN_API_KEY", "MODELSTUDIO_API_KEY", "DASHSCOPE_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(1_000_000),
            &[
                "Supports Qwen Cloud Coding Plan and Standard DashScope endpoints.",
                "Coding Plan endpoints omit models that are only available on Standard endpoints.",
            ],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "coding-plan-global",
                &["QWEN_API_KEY", "MODELSTUDIO_API_KEY", "DASHSCOPE_API_KEY"],
                &["QWEN_ENDPOINT=coding-global", "provider.qwen.endpoint=coding-global"],
                &["Uses https://coding-intl.dashscope.aliyuncs.com/v1."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "coding-plan-cn",
                &["QWEN_API_KEY", "MODELSTUDIO_API_KEY", "DASHSCOPE_API_KEY"],
                &["QWEN_ENDPOINT=coding-cn", "provider.qwen.endpoint=coding-cn"],
                &["Uses https://coding.dashscope.aliyuncs.com/v1."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "standard-global",
                &["QWEN_API_KEY", "MODELSTUDIO_API_KEY", "DASHSCOPE_API_KEY"],
                &["QWEN_ENDPOINT=standard-global", "provider.qwen.endpoint=standard-global"],
                &["Uses https://dashscope-intl.aliyuncs.com/compatible-mode/v1."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "standard-cn",
                &["QWEN_API_KEY", "MODELSTUDIO_API_KEY", "DASHSCOPE_API_KEY"],
                &["QWEN_ENDPOINT=standard-cn", "provider.qwen.endpoint=standard-cn"],
                &["Uses https://dashscope.aliyuncs.com/compatible-mode/v1."],
            ),
        ]),
        spec(
            "zai",
            "Z.AI",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://api.z.ai/api/paas/v4"),
            "glm-5.1",
            &["ZAI_API_KEY", "Z_AI_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_max_context_window(202_800),
            &[
                "Supports Z.AI general and Coding Plan endpoints.",
                "Coding Plan endpoints can be forced explicitly or detected from live probes.",
            ],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "coding-plan-global",
                &["ZAI_API_KEY", "Z_AI_API_KEY"],
                &["ZAI_ENDPOINT=coding-global", "provider.zai.endpoint=coding-global"],
                &["Uses https://api.z.ai/api/coding/paas/v4."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "coding-plan-cn",
                &["ZAI_API_KEY", "Z_AI_API_KEY"],
                &["ZAI_ENDPOINT=coding-cn", "provider.zai.endpoint=coding-cn"],
                &["Uses https://open.bigmodel.cn/api/coding/paas/v4."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "global",
                &["ZAI_API_KEY", "Z_AI_API_KEY"],
                &["ZAI_ENDPOINT=global", "provider.zai.endpoint=global"],
                &["Uses https://api.z.ai/api/paas/v4."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "cn",
                &["ZAI_API_KEY", "Z_AI_API_KEY"],
                &["ZAI_ENDPOINT=cn", "provider.zai.endpoint=cn"],
                &["Uses https://open.bigmodel.cn/api/paas/v4."],
            ),
        ]),
        openai_compatible_spec(
            "vercel-ai-gateway",
            "Vercel AI Gateway",
            "anthropic/claude-opus-4.6",
        ),
        spec(
            "cloudflare-ai-gateway",
            "Cloudflare AI Gateway",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            None,
            "openai/gpt-5-mini",
            &["CLOUDFLARE_AI_GATEWAY_API_KEY"],
            &["CLOUDFLARE_ACCOUNT_ID", "CLOUDFLARE_GATEWAY_ID"],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(200_000),
            &[
                "Uses the Cloudflare AI Gateway OpenAI-compatible /compat/chat/completions endpoint.",
                "Sends the upstream provider credential as the standard bearer token for compatibility mode.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["CLOUDFLARE_AI_GATEWAY_API_KEY"],
            &[
                "CLOUDFLARE_ACCOUNT_ID",
                "CLOUDFLARE_GATEWAY_ID",
                "provider.cloudflare-ai-gateway.baseUrl",
            ],
            &[
                "Builds https://gateway.ai.cloudflare.com/v1/{account}/{gateway}/compat when no explicit base URL override is present.",
                "Gateway id defaults to default when only the account id is configured.",
            ],
        )]),
        openai_compatible_spec("cloudflare-workers-ai", "Cloudflare Workers AI", "@cf/meta/llama-3.1-70b-instruct"),
        spec(
            "sap-ai-core",
            "SAP AI Core",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ServiceKey,
            None,
            "anthropic--claude-sonnet-4-6",
            &["AICORE_SERVICE_KEY"],
            &["AICORE_DEPLOYMENT_ID", "AICORE_RESOURCE_GROUP"],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(200_000),
            &[
                "Uses SAP AI Core service-key auth and deployment-scoped OpenAI-compatible inference endpoints.",
                "Resolves bearer tokens from the service key before calling AI_API_URL/v2/inference/deployments/{deployment_id}.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ServiceKey,
            "service-key",
            &["AICORE_SERVICE_KEY"],
            &[
                "AICORE_DEPLOYMENT_ID",
                "AICORE_RESOURCE_GROUP",
                "provider.sap-ai-core.baseUrl",
            ],
            &[
                "Parses clientid, clientsecret, url, and serviceurls.AI_API_URL from the service key JSON.",
                "Mints OAuth bearer tokens through the SAP AI Core OAuth endpoint.",
            ],
        )]),
        openai_compatible_alias_spec("deepseek", "DeepSeek", "deepseek-chat"),
        openai_compatible_alias_spec("mistral", "Mistral", "mistral-large-latest"),
        openai_compatible_alias_spec("moonshot", "Moonshot", "moonshot-v1-128k"),
        openai_compatible_alias_spec("moonshotai", "Moonshot AI", "moonshot-v1-128k"),
        spec(
            "minimax",
            "MiniMax",
            ModelProviderFamily::AnthropicMessages,
            ProviderAuthKind::ApiKey,
            Some("https://api.minimax.io/anthropic"),
            "MiniMax-M2.7",
            &["MINIMAX_API_KEY"],
            &["MINIMAX_REGION", "MINIMAX_API_HOST"],
            &[],
            ModelCapabilities::anthropic_messages()
                .with_image_input(true)
                .with_server_side_conversation_state(false),
            &[
                "Uses the Anthropic-compatible MiniMax M2.7 endpoint.",
                "Supports global and CN Coding Plan API-key routing.",
            ],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "api-global",
                &["MINIMAX_API_KEY"],
                &["MINIMAX_REGION=global", "provider.minimax.region=global"],
                &["Uses https://api.minimax.io/anthropic."],
            ),
            auth_strategy(
                ProviderAuthKind::ApiKey,
                "api-cn",
                &["MINIMAX_API_KEY"],
                &["MINIMAX_REGION=cn", "provider.minimax.region=cn"],
                &["Uses https://api.minimaxi.com/anthropic."],
            ),
        ]),
        openai_compatible_alias_spec("litellm", "LiteLLM", "anthropic/claude-opus-4.6"),
        openai_compatible_alias_spec("huggingface", "Hugging Face", "Qwen/Qwen3-Coder-480B-A35B-Instruct"),
        openai_compatible_alias_spec("together", "Together AI", "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8"),
        openai_compatible_alias_spec("togetherai", "Together AI", "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8"),
        openai_compatible_alias_spec("venice", "Venice AI", "venice-uncensored"),
        openai_compatible_alias_spec("qianfan", "Qianfan", "ernie-4.5-8k"),
        openai_compatible_alias_spec("synthetic", "Synthetic", "Synthetic-1"),
        openai_compatible_alias_spec("xiaomi", "Xiaomi", "mimo-v2-flash"),
        spec(
            "kimi",
            "Kimi Code",
            ModelProviderFamily::AnthropicMessages,
            ProviderAuthKind::ApiKey,
            Some("https://api.kimi.com/coding"),
            "kimi-code",
            &["KIMI_API_KEY", "KIMICODE_API_KEY"],
            &[],
            &[("User-Agent", "claude-code/0.1.0")],
            ModelCapabilities::anthropic_messages()
                .with_image_input(true)
                .with_max_context_window(262_144),
            &[
                "Uses the dedicated Kimi coding endpoint.",
                "Accepts the Kimi Code subscription API key.",
            ],
        ),
        spec(
            "azure",
            "Azure OpenAI",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            None,
            "gpt-4.1",
            &["AZURE_OPENAI_API_KEY", "AZURE_API_KEY"],
            &["AZURE_RESOURCE_NAME", "AZURE_OPENAI_DEPLOYMENT"],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(200_000),
            &[
                "Uses the Azure OpenAI v1 endpoint with api-key authentication.",
                "The model field should resolve to the Azure deployment name.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["AZURE_OPENAI_API_KEY", "AZURE_API_KEY"],
            &[
                "AZURE_RESOURCE_NAME",
                "AZURE_OPENAI_DEPLOYMENT",
                "provider.azure.baseUrl",
            ],
            &[
                "Builds https://{resource}.openai.azure.com/openai/v1 when no explicit base URL override is present.",
                "Sends API keys with the api-key header instead of Authorization.",
            ],
        )]),
        spec(
            "azure-cognitive-services",
            "Azure Cognitive Services",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            None,
            "gpt-4.1",
            &["AZURE_API_KEY"],
            &[
                "AZURE_COGNITIVE_SERVICES_RESOURCE_NAME",
                "AZURE_COGNITIVE_SERVICES_DEPLOYMENT",
            ],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(200_000),
            &[
                "Uses the Azure Cognitive Services OpenAI-compatible v1 endpoint with api-key authentication.",
                "The model field should resolve to the Azure deployment name.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["AZURE_API_KEY"],
            &[
                "AZURE_COGNITIVE_SERVICES_RESOURCE_NAME",
                "AZURE_COGNITIVE_SERVICES_DEPLOYMENT",
                "provider.azure-cognitive-services.baseUrl",
            ],
            &[
                "Builds https://{resource}.cognitiveservices.azure.com/openai/v1 when no explicit base URL override is present.",
                "Sends API keys with the api-key header instead of Authorization.",
            ],
        )]),
        spec(
            "copilot-proxy",
            "Copilot Proxy",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::Local,
            Some("http://localhost:3000/v1"),
            "gpt-4.1",
            &[],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(200_000),
            &[
                "Targets a local OpenAI-compatible Copilot proxy endpoint.",
                "Defaults to http://localhost:3000/v1 and can be overridden for other local proxy ports.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::Local,
            "local",
            &[],
            &["provider.copilot-proxy.baseUrl"],
            &["Optional bearer tokens can still be supplied through a provider override when the local proxy requires them."],
        )]),
        spec(
            "microsoft-foundry",
            "Microsoft Foundry",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            None,
            "gpt-4.1",
            &["MICROSOFT_FOUNDRY_API_KEY", "AZURE_API_KEY"],
            &["MICROSOFT_FOUNDRY_RESOURCE_NAME", "MICROSOFT_FOUNDRY_DEPLOYMENT"],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(200_000),
            &[
                "Uses the Microsoft Foundry OpenAI-compatible v1 endpoint on services.ai.azure.com.",
                "The model field should resolve to the Foundry deployment or route name.",
            ],
        )
        .with_auth_strategies(vec![auth_strategy(
            ProviderAuthKind::ApiKey,
            "api-key",
            &["MICROSOFT_FOUNDRY_API_KEY", "AZURE_API_KEY"],
            &[
                "MICROSOFT_FOUNDRY_RESOURCE_NAME",
                "MICROSOFT_FOUNDRY_DEPLOYMENT",
                "provider.microsoft-foundry.baseUrl",
            ],
            &[
                "Builds https://{resource}.services.ai.azure.com/openai/v1 when no explicit base URL override is present.",
                "Sends API keys with the api-key header instead of Authorization.",
            ],
        )]),
        openai_compatible_alias_spec("302ai", "302.AI", "gpt-4.1-mini"),
        openai_compatible_alias_spec("arcee", "Arcee", "arcee-ai/coder-large"),
        openai_compatible_alias_spec("baseten", "Baseten", "deepseek-ai/DeepSeek-R1"),
        spec(
            "byteplus",
            "BytePlus",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://ark.ap-southeast.bytepluses.com/api/v3"),
            "seed-1-8-251228",
            &["BYTEPLUS_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(256_000),
            &["Uses the BytePlus standard OpenAI-compatible API surface."],
        ),
        spec(
            "byteplus-plan",
            "BytePlus Plan",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://ark.ap-southeast.bytepluses.com/api/coding/v3"),
            "ark-code-latest",
            &["BYTEPLUS_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_max_context_window(256_000),
            &["Uses the BytePlus coding-plan OpenAI-compatible API surface."],
        ),
        openai_compatible_alias_spec("cerebras", "Cerebras", "llama-4-scout"),
        openai_compatible_alias_spec("chutes", "Chutes", "deepseek-ai/DeepSeek-V3"),
        openai_compatible_alias_spec("cohere", "Cohere", "command-a"),
        openai_compatible_alias_spec("cortecs", "Cortecs", "moonshotai/kimi-k2-instruct"),
        openai_compatible_alias_spec("deepinfra", "DeepInfra", "meta-llama/Llama-3.1-70B-Instruct"),
        openai_compatible_alias_spec("firmware", "Firmware", "gpt-4.1-mini"),
        openai_compatible_alias_spec("fireworks", "Fireworks AI", "accounts/fireworks/models/llama-v3p1-70b-instruct"),
        openai_compatible_alias_spec("fireworks-ai", "Fireworks AI", "accounts/fireworks/models/llama-v3p1-70b-instruct"),
        openai_compatible_alias_spec("groq", "Groq", "llama-3.3-70b-versatile"),
        openai_compatible_alias_spec("helicone", "Helicone", "gpt-4.1-mini"),
        openai_compatible_alias_spec("io-net", "IO.NET", "meta-llama/Llama-3.3-70B-Instruct"),
        openai_compatible_alias_spec("kilo", "Kilo", "gpt-4.1-mini"),
        openai_compatible_alias_spec("kilocode", "KiloCode", "kilocode/kilo-agent"),
        openai_compatible_spec("llama.cpp", "llama.cpp", "meta-llama/Llama-3.1-8B-Instruct"),
        openai_compatible_spec("lmstudio", "LM Studio", "google/gemma-3n-e4b"),
        spec(
            "minimax-portal",
            "MiniMax Portal",
            ModelProviderFamily::AnthropicMessages,
            ProviderAuthKind::OAuth,
            Some("https://api.minimax.io/anthropic"),
            "MiniMax-M2.7",
            &["MINIMAX_OAUTH_TOKEN", "MINIMAX_API_KEY"],
            &["MINIMAX_REGION", "provider.minimax-portal.region"],
            &[],
            ModelCapabilities::anthropic_messages()
                .with_image_input(true)
                .with_server_side_conversation_state(false),
            &[
                "Uses MiniMax Portal OAuth against the Anthropic-compatible endpoint.",
                "Supports global and CN device-code-style OAuth login.",
            ],
        )
        .with_auth_strategies(vec![
            auth_strategy(
                ProviderAuthKind::OAuth,
                "oauth-global",
                &["MINIMAX_OAUTH_TOKEN"],
                &["MINIMAX_REGION=global", "provider.minimax-portal.region=global"],
                &["Uses https://api.minimax.io."],
            ),
            auth_strategy(
                ProviderAuthKind::OAuth,
                "oauth-cn",
                &["MINIMAX_OAUTH_TOKEN"],
                &["MINIMAX_REGION=cn", "provider.minimax-portal.region=cn"],
                &["Uses https://api.minimaxi.com."],
            ),
        ]),
        openai_compatible_alias_spec("nebius-token-factory", "Nebius Token Factory", "moonshotai/kimi-k2-instruct"),
        openai_compatible_alias_spec("nebius", "Nebius Token Factory", "moonshotai/kimi-k2-instruct"),
        openai_compatible_alias_spec("nvidia", "NVIDIA", "meta/llama-3.1-70b-instruct"),
        openai_compatible_alias_spec("ollama-cloud", "Ollama Cloud", "gpt-oss:20b-cloud"),
        openai_compatible_alias_spec("ovhcloud-ai-endpoints", "OVHcloud AI Endpoints", "gpt-oss-120b"),
        openai_compatible_alias_spec("ovhcloud", "OVHcloud AI Endpoints", "gpt-oss-120b"),
        openai_compatible_alias_spec("poe", "Poe", "anthropic/claude-sonnet-4-6"),
        openai_compatible_alias_spec("scaleway", "Scaleway", "devstral-2-123b-instruct-2512"),
        openai_compatible_alias_spec("stackit", "STACKIT", "Llama-3.3-70B-Instruct"),
        spec(
            "stepfun",
            "StepFun",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://api.stepfun.ai/v1"),
            "step-3.5-flash",
            &["STEPFUN_API_KEY"],
            &["STEPFUN_REGION"],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_max_context_window(262_144),
            &["Uses the StepFun standard OpenAI-compatible API surface."],
        ),
        spec(
            "stepfun-plan",
            "StepFun Plan",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://api.stepfun.ai/step_plan/v1"),
            "step-3.5-flash",
            &["STEPFUN_API_KEY"],
            &["STEPFUN_REGION"],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_max_context_window(262_144),
            &["Uses the StepFun Step Plan OpenAI-compatible API surface."],
        ),
        spec(
            "volcengine",
            "Volcengine",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://ark.cn-beijing.volces.com/api/v3"),
            "doubao-seed-1-8-251228",
            &["VOLCANO_ENGINE_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_image_input(true)
                .with_max_context_window(256_000),
            &["Uses the Volcengine standard OpenAI-compatible API surface."],
        ),
        spec(
            "volcengine-plan",
            "Volcengine Plan",
            ModelProviderFamily::OpenAiCompatible,
            ProviderAuthKind::ApiKey,
            Some("https://ark.cn-beijing.volces.com/api/coding/v3"),
            "ark-code-latest",
            &["VOLCANO_ENGINE_API_KEY"],
            &[],
            &[],
            ModelCapabilities::openai_compatible()
                .with_tool_call_streaming(true)
                .with_max_context_window(256_000),
            &["Uses the Volcengine coding-plan OpenAI-compatible API surface."],
        ),
        openai_compatible_spec("vercel", "Vercel AI Gateway", "anthropic/claude-sonnet-4"),
        openai_compatible_alias_spec("zenmux", "ZenMux", "anthropic/claude-sonnet-4-6"),
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
        "github-copilot" => &["GITHUB_COPILOT_TOKEN"][..],
        "gitlab" => &["GITLAB_TOKEN"][..],
        "cloudflare-workers-ai" => &["CLOUDFLARE_API_KEY"][..],
        "xai" => &["XAI_API_KEY"][..],
        "mistral" => &["MISTRAL_API_KEY"][..],
        "moonshot" | "moonshotai" => &["MOONSHOT_API_KEY"][..],
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

fn openai_compatible_alias_spec(
    id: &'static str,
    display_name: &'static str,
    default_model: &'static str,
) -> ProviderSpec {
    openai_compatible_spec(id, display_name, default_model).with_notes(&[
        "Compatibility alias routed through the generic OpenAI-compatible adapter.",
        "Does not claim provider-native auth, model discovery, or custom transport behavior beyond the compatibility API surface.",
    ])
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
