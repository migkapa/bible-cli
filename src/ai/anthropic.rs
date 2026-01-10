use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::Stream;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::ai::{require_env, ProviderRequest, StreamEvent};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone)]
pub struct AnthropicClient {
    api_key: String,
}

impl AnthropicClient {
    pub fn new() -> Result<Self> {
        let api_key = require_env("ANTHROPIC_API_KEY")?;
        Ok(Self { api_key })
    }

    pub fn stream_request(
        &self,
        request: &ProviderRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send + '_>> {
        let api_key = self.api_key.clone();
        let anthropic_request = AnthropicMessageRequest::from_request(request);

        Box::pin(async_stream::try_stream! {
            let mut headers = HeaderMap::new();
            headers.insert("x-api-key", HeaderValue::from_str(&api_key)?);
            headers.insert("anthropic-version", HeaderValue::from_static(ANTHROPIC_VERSION));
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

            let http = reqwest::Client::builder()
                .timeout(DEFAULT_TIMEOUT)
                .build()?;

            let response = http
                .post(ANTHROPIC_URL)
                .headers(headers)
                .json(&anthropic_request)
                .send()
                .await
                .context("Failed to send Anthropic request")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                Err(anyhow!("Anthropic request failed with status {}: {}", status, body.trim()))?;
                return;
            }

            yield StreamEvent::Start;

            let mut buffer = String::new();
            let mut stream = response.bytes_stream();

            use futures::StreamExt;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.context("Failed to read chunk")?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    // Anthropic uses "event: type" followed by "data: {...}"
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(event) = serde_json::from_str::<AnthropicStreamEvent>(data) {
                            match event.event_type.as_str() {
                                "content_block_delta" => {
                                    if let Some(delta) = event.delta {
                                        if let Some(text) = delta.text {
                                            if !text.is_empty() {
                                                yield StreamEvent::Delta(text);
                                            }
                                        }
                                    }
                                }
                                "message_stop" => {
                                    yield StreamEvent::Done;
                                    return;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            yield StreamEvent::Done;
        })
    }
}

#[derive(Debug, Serialize)]
struct AnthropicMessageRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    stream: bool,
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
            stream: true,
        }
    }
}

#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<AnthropicDelta>,
}

#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    text: Option<String>,
}
