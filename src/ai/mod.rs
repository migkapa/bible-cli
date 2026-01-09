use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub trait ProviderClient {
    fn send_request(&self, request: &ProviderRequest) -> Result<ProviderResponse>;
}

#[derive(Debug, Clone)]
pub struct ProviderRequest {
    pub model: String,
    pub system: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    http: Client,
    api_key: String,
}

impl OpenAiClient {
    pub fn new() -> Result<Self> {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Result<Self> {
        let api_key = require_env("OPENAI_API_KEY")?;
        let http = Client::builder().timeout(timeout).build()?;
        Ok(Self { http, api_key })
    }
}

impl ProviderClient for OpenAiClient {
    fn send_request(&self, request: &ProviderRequest) -> Result<ProviderResponse> {
        let openai_request = OpenAiChatCompletionRequest::from_request(request);
        let response = self
            .http
            .post(OPENAI_URL)
            .header(AUTHORIZATION, format!("Bearer {}", self.api_key))
            .header(CONTENT_TYPE, "application/json")
            .json(&openai_request)
            .send()
            .context("Failed to send OpenAI request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow!(
                "OpenAI request failed with status {}: {}",
                status,
                body.trim()
            ));
        }

        let data: OpenAiChatCompletionResponse = response
            .json()
            .context("Failed to deserialize OpenAI response")?;
        let message = data
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .context("OpenAI response contained no message content")?;

        Ok(ProviderResponse { content: message })
    }
}

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    http: Client,
    api_key: String,
}

impl AnthropicClient {
    pub fn new() -> Result<Self> {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    pub fn with_timeout(timeout: Duration) -> Result<Self> {
        let api_key = require_env("ANTHROPIC_API_KEY")?;
        let http = Client::builder().timeout(timeout).build()?;
        Ok(Self { http, api_key })
    }

    fn headers(&self) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_str(&self.api_key)?);
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static(ANTHROPIC_VERSION),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }
}

impl ProviderClient for AnthropicClient {
    fn send_request(&self, request: &ProviderRequest) -> Result<ProviderResponse> {
        let anthropic_request = AnthropicMessageRequest::from_request(request);
        let response = self
            .http
            .post(ANTHROPIC_URL)
            .headers(self.headers()?)
            .json(&anthropic_request)
            .send()
            .context("Failed to send Anthropic request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            return Err(anyhow!(
                "Anthropic request failed with status {}: {}",
                status,
                body.trim()
            ));
        }

        let data: AnthropicMessageResponse = response
            .json()
            .context("Failed to deserialize Anthropic response")?;
        let message = data
            .content
            .into_iter()
            .find_map(|block| block.text)
            .context("Anthropic response contained no message content")?;

        Ok(ProviderResponse { content: message })
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

impl OpenAiChatCompletionRequest {
    fn from_request(request: &ProviderRequest) -> Self {
        let mut messages = Vec::new();
        if let Some(system) = &request.system {
            messages.push(OpenAiMessage::new("system", system));
        }
        messages.extend(
            request
                .messages
                .iter()
                .map(|message| OpenAiMessage::new(message.role.as_str(), message.content.as_str())),
        );

        Self {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

impl OpenAiMessage {
    fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Serialize)]
struct AnthropicMessageRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

impl AnthropicMessageRequest {
    fn from_request(request: &ProviderRequest) -> Self {
        let messages = request
            .messages
            .iter()
            .map(|message| AnthropicMessage {
                role: message.role.clone(),
                content: message.content.clone(),
            })
            .collect();

        Self {
            model: request.model.clone(),
            max_tokens: request.max_tokens.unwrap_or(256),
            messages,
            system: request.system.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageResponse {
    content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContentBlock {
    text: Option<String>,
}

fn require_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("Missing required environment variable: {}", key))
}
