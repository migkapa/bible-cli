use std::pin::Pin;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use futures::Stream;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::ai::{require_env, ProviderRequest, StreamEvent};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    api_key: String,
}

impl OpenAiClient {
    pub fn new() -> Result<Self> {
        let api_key = require_env("OPENAI_API_KEY")?;
        Ok(Self { api_key })
    }

    pub fn stream_request(
        &self,
        request: &ProviderRequest,
    ) -> Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send + '_>> {
        let api_key = self.api_key.clone();
        let openai_request = OpenAiChatCompletionRequest::from_request(request);

        Box::pin(async_stream::try_stream! {
            let http = reqwest::Client::builder()
                .timeout(DEFAULT_TIMEOUT)
                .build()?;

            let response = http
                .post(OPENAI_URL)
                .header(AUTHORIZATION, format!("Bearer {}", api_key))
                .header(CONTENT_TYPE, "application/json")
                .json(&openai_request)
                .send()
                .await
                .context("Failed to send OpenAI request")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                Err(anyhow!("OpenAI request failed with status {}: {}", status, body.trim()))?;
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

                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            yield StreamEvent::Done;
                            return;
                        }

                        if let Ok(parsed) = serde_json::from_str::<OpenAiStreamChunk>(data) {
                            if let Some(choice) = parsed.choices.first() {
                                if let Some(content) = &choice.delta.content {
                                    if !content.is_empty() {
                                        yield StreamEvent::Delta(content.clone());
                                    }
                                }
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
struct OpenAiChatCompletionRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
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
                .map(|message| OpenAiMessage::new(&message.role, &message.content)),
        );

        Self {
            model: request.model.clone(),
            messages,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: true,
        }
    }
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Default, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiDelta,
}
