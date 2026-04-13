//! Builtin provider registry derived from the OpenClaw and OpenCode provider surfaces.

use crate::model::capabilities::ModelCapabilities;
use crate::model::family::ModelProviderFamily;
use crate::model::provider_spec::{ProviderAuthKind, ProviderSpec};
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
        ),
        spec(
            "anthropic",
            "Anthropic",
            ModelProviderFamily::AnthropicMessages,
            ProviderAuthKind::Hybrid,
            Some("https://api.anthropic.com"),
            "claude-sonnet-4-5",
            &["ANTHROPIC_API_KEY"],
            &[],
            &[(
                "anthropic-beta",
                "interleaved-thinking-2025-05-14,fine-grained-tool-streaming-2025-05-14",
            )],
            ModelCapabilities::anthropic_messages(),
            &["Anthropic Messages family.", "Beta headers mirror OpenCode's streaming/tool behavior."],
        ),
        spec(
            "google",
            "Google",
            ModelProviderFamily::GoogleGenerativeAi,
            ProviderAuthKind::ApiKey,
            Some("https://generativelanguage.googleapis.com"),
            "gemini-2.5-pro",
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
            "gemini-2.5-pro",
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
            "anthropic.claude-sonnet-4-5",
            &["AWS_BEARER_TOKEN_BEDROCK"],
            &["AWS_REGION", "AWS_PROFILE"],
            &[],
            ModelCapabilities::bedrock_converse(),
            &["Uses Bedrock Converse/ConverseStream semantics.", "Model-prefix logic mirrors OpenCode's region handling."],
        ),
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
            "anthropic/claude-sonnet-4-5",
        ),
        openai_compatible_spec("cloudflare-workers-ai", "Cloudflare Workers AI", "@cf/meta/llama-3.1-70b-instruct"),
        openai_compatible_spec("sap-ai-core", "SAP AI Core", "anthropic/claude-sonnet-4-5"),
        openai_compatible_spec("github-copilot", "GitHub Copilot", "gpt-4o"),
        openai_compatible_spec("gitlab", "GitLab Duo", "duo-chat-sonnet-4-5"),
        openai_compatible_spec("deepseek", "DeepSeek", "deepseek-chat"),
        openai_compatible_spec("xai", "xAI", "grok-4"),
        openai_compatible_spec("mistral", "Mistral", "mistral-large-latest"),
        openai_compatible_spec("moonshot", "Moonshot", "moonshot-v1-128k"),
        openai_compatible_spec("minimax", "MiniMax", "MiniMax-M2.7"),
        openai_compatible_spec("litellm", "LiteLLM", "anthropic/claude-opus-4.6"),
        openai_compatible_spec("huggingface", "Hugging Face", "Qwen/Qwen3-Coder-480B-A35B-Instruct"),
        openai_compatible_spec("together", "Together AI", "meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8"),
        openai_compatible_spec("venice", "Venice AI", "venice-uncensored"),
        openai_compatible_spec("qianfan", "Qianfan", "ernie-4.5-8k"),
        openai_compatible_spec("synthetic", "Synthetic", "Synthetic-1"),
        openai_compatible_spec("xiaomi", "Xiaomi", "mimo-v2-flash"),
        openai_compatible_spec("kimi", "Kimi", "kimi-k2.5"),
        openai_compatible_spec("azure", "Azure OpenAI", "gpt-4.1"),
        openai_compatible_spec("azure-cognitive-services", "Azure Cognitive Services", "gpt-4.1"),
        openai_compatible_spec("anthropic-vertex", "Anthropic Vertex", "claude-sonnet-4-5"),
        openai_compatible_spec("google-vertex-anthropic", "Google Vertex Anthropic", "claude-sonnet-4-5"),
        openai_compatible_spec("amazon-bedrock-mantle", "Amazon Bedrock Mantle", "anthropic.claude-sonnet-4-5"),
        openai_compatible_spec("google-gemini-cli", "Google Gemini CLI", "gemini-2.5-pro"),
        openai_compatible_spec("codex", "Codex", "gpt-5.4"),
        openai_compatible_spec("openai-codex", "OpenAI Codex", "gpt-5.4"),
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
        openai_compatible_spec("groq", "Groq", "llama-3.3-70b-versatile"),
        openai_compatible_spec("helicone", "Helicone", "gpt-4.1-mini"),
        openai_compatible_spec("io-net", "IO.NET", "meta-llama/Llama-3.3-70B-Instruct"),
        openai_compatible_spec("kilo", "Kilo", "gpt-4.1-mini"),
        openai_compatible_spec("kilocode", "KiloCode", "kilocode/kilo-agent"),
        openai_compatible_spec("llama.cpp", "llama.cpp", "meta-llama/Llama-3.1-8B-Instruct"),
        openai_compatible_spec("lmstudio", "LM Studio", "google/gemma-3n-e4b"),
        openai_compatible_spec("minimax-portal", "MiniMax Portal", "MiniMax-M2.7"),
        openai_compatible_spec("nebius-token-factory", "Nebius Token Factory", "moonshotai/kimi-k2-instruct"),
        openai_compatible_spec("nvidia", "NVIDIA", "meta/llama-3.1-70b-instruct"),
        openai_compatible_spec("ollama-cloud", "Ollama Cloud", "gpt-oss:20b-cloud"),
        openai_compatible_spec("ovhcloud-ai-endpoints", "OVHcloud AI Endpoints", "gpt-oss-120b"),
        openai_compatible_spec("perplexity", "Perplexity", "sonar"),
        openai_compatible_spec("poe", "Poe", "anthropic/claude-sonnet-4-5"),
        openai_compatible_spec("scaleway", "Scaleway", "devstral-2-123b-instruct-2512"),
        openai_compatible_spec("stackit", "STACKIT", "Llama-3.3-70B-Instruct"),
        openai_compatible_spec("stepfun", "StepFun", "step-2"),
        openai_compatible_spec("stepfun-plan", "StepFun Plan", "step-2-thinking"),
        openai_compatible_spec("volcengine", "Volcengine", "doubao-1.5-pro"),
        openai_compatible_spec("volcengine-plan", "Volcengine Plan", "doubao-1.5-thinking-pro"),
        openai_compatible_spec("zenmux", "ZenMux", "anthropic/claude-sonnet-4-5"),
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
        "vercel-ai-gateway" => Some("https://ai-gateway.vercel.sh/v1"),
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
        "moonshot" => &["MOONSHOT_API_KEY"][..],
        "minimax" | "minimax-portal" => &["MINIMAX_API_KEY"][..],
        "together" => &["TOGETHER_API_KEY"][..],
        "venice" => &["VENICE_API_KEY"][..],
        "qianfan" => &["QIANFAN_API_KEY"][..],
        "synthetic" => &["SYNTHETIC_API_KEY"][..],
        "xiaomi" => &["XIAOMI_API_KEY"][..],
        "kimi" => &["KIMI_API_KEY"][..],
        "huggingface" => &["HUGGINGFACE_HUB_TOKEN"][..],
        "litellm" => &["LITELLM_API_KEY"][..],
        "deepseek" => &["DEEPSEEK_API_KEY"][..],
        "cerebras" => &["CEREBRAS_API_KEY"][..],
        "cohere" => &["COHERE_API_KEY"][..],
        "groq" => &["GROQ_API_KEY"][..],
        "zenmux" => &["ZENMUX_API_KEY"][..],
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
