//! Provider-family groupings used to normalize runtime behavior.

/// The transport and protocol family for a model provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ModelProviderFamily {
    /// OpenAI Responses-compatible providers.
    OpenAiResponses,
    /// OpenAI-compatible chat/completions providers and gateways.
    OpenAiCompatible,
    /// Anthropic Messages-compatible providers.
    AnthropicMessages,
    /// Google Generative AI providers.
    GoogleGenerativeAi,
    /// Google Vertex providers for Google-native models.
    GoogleVertex,
    /// Google Vertex providers for Anthropic-hosted models.
    GoogleVertexAnthropic,
    /// AWS Bedrock Converse/ConverseStream providers.
    AwsBedrockConverse,
    /// GitHub Copilot-specific transport and auth behavior.
    GitHubCopilot,
    /// GitLab Duo/GitLab AI Gateway behavior.
    GitLabDuo,
}

impl ModelProviderFamily {
    /// Returns a concise transport label that is useful in diagnostics.
    pub fn transport_label(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai-responses",
            Self::OpenAiCompatible => "openai-compatible",
            Self::AnthropicMessages => "anthropic-messages",
            Self::GoogleGenerativeAi => "google-generative-ai",
            Self::GoogleVertex => "google-vertex",
            Self::GoogleVertexAnthropic => "google-vertex-anthropic",
            Self::AwsBedrockConverse => "aws-bedrock-converse",
            Self::GitHubCopilot => "github-copilot",
            Self::GitLabDuo => "gitlab-duo",
        }
    }
}
