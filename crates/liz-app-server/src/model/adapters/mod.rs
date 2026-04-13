//! Family adapters used by the provider-aware model gateway.

mod anthropic;
mod bedrock;
mod google;
mod openai_style;

pub use anthropic::AnthropicAdapter;
pub use bedrock::AwsBedrockAdapter;
pub use google::GoogleAdapter;
pub use openai_style::OpenAiStyleAdapter;
