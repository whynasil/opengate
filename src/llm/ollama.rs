use futures::StreamExt;
use reqwest::Client;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{ChatStream, LlmBackend, LlmError, LlmMessage};

pub struct OllamaBackend {
    host: String,
    model: String,
    client: Client,
}

impl OllamaBackend {
    pub fn new(host: impl Into<String>, model: impl Into<String>) -> Self {
        Self { host: host.into(), model: model.into(), client: Client::new() }
    }
}

impl LlmBackend for OllamaBackend {
    async fn chat(&self, messages: Vec<LlmMessage>) -> Result<String, LlmError> {
        let url = format!("{}/api/chat", self.host.trim_end_matches('/'));

        let ollama_messages: Vec<serde_json::Value> = messages
            .into_iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let body = serde_json::json!({
            "model": self.model,
            "messages": ollama_messages,
            "stream": false,
        });

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Api(format!("Request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let text = response.text().await.unwrap_or_default();
            return Err(LlmError::Api(format!("API returned {status}: {text}")));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| LlmError::Api(format!("Failed to parse response: {e}")))?;

        json["message"]["content"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| LlmError::Api("No content in response".to_string()))
    }

    async fn stream_chat(&self, messages: Vec<LlmMessage>) -> Result<ChatStream, LlmError> {
        let url = format!("{}/api/chat", self.host.trim_end_matches('/'));

        let ollama_messages: Vec<serde_json::Value> = messages
            .into_iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                })
            })
            .collect();

        let body = serde_json::json!({
            "model": self.model,
            "messages": ollama_messages,
            "stream": true,
        });

        let response = self
            .client
            .post(&url)
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

                        // Ollama NDJSON: one JSON object per line
                        while let Some(pos) = text_buf.find('\n') {
                            let line = text_buf[..pos].to_string();
                            text_buf = text_buf[pos + 1..].to_string();

                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }

                            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                                if let Some(content) = parsed["message"]["content"].as_str()
                                    && !content.is_empty()
                                {
                                    let _ = tx.send(Ok(content.to_string()));
                                }
                                if parsed["done"].as_bool() == Some(true) {
                                    return;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ollama_backend_creation() {
        let backend = OllamaBackend::new("http://localhost:11434", "llama3");
        assert_eq!(backend.host, "http://localhost:11434");
        assert_eq!(backend.model, "llama3");
    }
}
