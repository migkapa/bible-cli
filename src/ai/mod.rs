mod anthropic;
mod openai;

use std::pin::Pin;

use anyhow::{bail, Context, Result};
use futures::Stream;

pub use anthropic::AnthropicClient;
pub use openai::OpenAiClient;

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Start,
    Delta(String),
    Done,
}

pub enum AiProvider {
    OpenAi(OpenAiClient),
    Anthropic(AnthropicClient),
}

impl AiProvider {
    pub fn from_name(name: &str) -> Result<Self> {
        match name.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAi(OpenAiClient::new()?)),
            "anthropic" => Ok(Self::Anthropic(AnthropicClient::new()?)),
            _ => bail!("Unknown provider: {}", name),
        }
    }

    pub fn stream_request(
        &self,
        request: &ProviderRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send + '_>> {
        match self {
            Self::OpenAi(client) => client.stream_request(request),
            Self::Anthropic(client) => client.stream_request(request),
        }
    }
}

pub fn require_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Missing required environment variable: {}", key))
}
