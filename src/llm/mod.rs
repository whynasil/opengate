use std::future::Future;
use std::pin::Pin;

pub mod anthropic;
pub mod ollama;

use futures::Stream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_stream::wrappers::UnboundedReceiverStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: String,
    pub content: String,
}

#[derive(Error, Debug)]
pub enum LlmError {
    #[error("API error: {0}")]
    Api(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Request timed out")]
    Timeout,
}

type ChatStream = Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>;

pub trait LlmBackend: Send + Sync {
    fn chat(
        &self,
        messages: Vec<LlmMessage>,
    ) -> impl Future<Output = Result<String, LlmError>> + Send;
    fn stream_chat(
        &self,
        messages: Vec<LlmMessage>,
    ) -> impl Future<Output = Result<ChatStream, LlmError>> + Send;
}

pub struct OpenAiBackend {
    api_key: String,
    base_url: String,
    model: String,
    client: Client,
}

impl OpenAiBackend {
    pub fn new(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            model: model.into(),
            client: Client::new(),
        }
    }
}

impl LlmBackend for OpenAiBackend {
    async fn chat(&self, messages: Vec<LlmMessage>) -> Result<String, LlmError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Api(format!("Request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("API returned {status}: {text}")));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| LlmError::Api(format!("Failed to parse response: {e}")))?;

        body["choices"][0]["message"]["content"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| LlmError::Api("No content in response".to_string()))
    }

    async fn stream_chat(
        &self,
        messages: Vec<LlmMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LlmError>> + Send>>, LlmError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Api(format!("Request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("API returned {status}: {text}")));
        }

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut byte_stream = response.bytes_stream();

        tokio::spawn(async move {
            let mut text_buf = String::new();

            while let Some(chunk_result) = byte_stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        text_buf.push_str(&String::from_utf8_lossy(&chunk));

                        while let Some(pos) = text_buf.find("\n\n") {
                            let event = text_buf[..pos].to_string();
                            text_buf = text_buf[pos + 2..].to_string();

                            for line in event.lines() {
                                if let Some(data) = line.strip_prefix("data: ") {
                                    let data = data.trim();
                                    if data == "[DONE]" {
                                        return;
                                    }
                                    if let Ok(parsed) =
                                        serde_json::from_str::<serde_json::Value>(data)
                                        && let Some(content) =
                                            parsed["choices"][0]["delta"]["content"].as_str()
                                    {
                                        let _ = tx.send(Ok(content.to_string()));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(LlmError::Api(format!("Stream error: {e}"))));
                        return;
                    }
                }
            }
        });

        Ok(Box::pin(UnboundedReceiverStream::new(rx)))
    }
}
