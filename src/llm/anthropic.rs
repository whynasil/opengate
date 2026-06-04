use futures::StreamExt;
use reqwest::Client;
use tokio_stream::wrappers::UnboundedReceiverStream;

use super::{ChatStream, LlmBackend, LlmError, LlmMessage};

pub struct AnthropicBackend {
    api_key: String,
    model: String,
    client: Client,
}

impl AnthropicBackend {
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self { api_key: api_key.into(), model: model.into(), client: Client::new() }
    }
}

impl LlmBackend for AnthropicBackend {
    async fn chat(&self, messages: Vec<LlmMessage>) -> Result<String, LlmError> {
        let url = "https://api.anthropic.com/v1/messages";

        // Anthropic requires system as top-level, rest as alternating user/assistant
        let (system_prompt, anthropic_messages) = convert_messages(messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": anthropic_messages,
        });
        if let Some(sys) = system_prompt {
            body["system"] = serde_json::Value::String(sys);
        }

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
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

        // Anthropic response: content[0].text
        json["content"][0]["text"]
            .as_str()
            .map(String::from)
            .ok_or_else(|| LlmError::Api("No content in response".to_string()))
    }

    async fn stream_chat(&self, messages: Vec<LlmMessage>) -> Result<ChatStream, LlmError> {
        let url = "https://api.anthropic.com/v1/messages";

        let (system_prompt, anthropic_messages) = convert_messages(messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "messages": anthropic_messages,
            "stream": true,
        });
        if let Some(sys) = system_prompt {
            body["system"] = serde_json::Value::String(sys);
        }

        let response = self
            .client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
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

                        while let Some(pos) = text_buf.find('\n') {
                            let event = text_buf[..pos].to_string();
                            text_buf = text_buf[pos + 1..].to_string();

                            let event = event.trim();
                            if event.is_empty() {
                                continue;
                            }

                            if let Some(data) = event.strip_prefix("data: ")
                                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data)
                            {
                                match parsed["type"].as_str() {
                                    Some("content_block_delta") => {
                                        if let Some(text) = parsed["delta"]["text"].as_str() {
                                            let _ = tx.send(Ok(text.to_string()));
                                        }
                                    }
                                    Some("message_stop") => return,
                                    _ => {}
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

/// Convert OpenAI-style role messages to Anthropic format.
/// Anthropic requires alternating user/assistant, system goes top-level.
fn convert_messages(messages: Vec<LlmMessage>) -> (Option<String>, Vec<serde_json::Value>) {
    let mut system_prompt: Option<String> = None;
    let mut anthropic_messages: Vec<serde_json::Value> = Vec::new();

    // Collect all system messages into one system prompt
    let system_texts: Vec<String> =
        messages.iter().filter(|m| m.role == "system").map(|m| m.content.clone()).collect();

    if !system_texts.is_empty() {
        system_prompt = Some(system_texts.join("\n\n"));
    }

    for msg in messages {
        match msg.role.as_str() {
            "system" => continue, // handled above
            "user" => {
                anthropic_messages.push(serde_json::json!({
                    "role": "user",
                    "content": msg.content,
                }));
            }
            "assistant" => {
                anthropic_messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": msg.content,
                }));
            }
            _ => {
                // Treat unknown roles as user
                anthropic_messages.push(serde_json::json!({
                    "role": "user",
                    "content": msg.content,
                }));
            }
        }
    }

    (system_prompt, anthropic_messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_messages_system_extraction() {
        let messages = vec![
            LlmMessage { role: "system".into(), content: "You are helpful.".into() },
            LlmMessage { role: "user".into(), content: "Hello".into() },
            LlmMessage { role: "assistant".into(), content: "Hi!".into() },
        ];

        let (system, converted) = convert_messages(messages);
        assert_eq!(system, Some("You are helpful.".to_string()));
        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0]["role"], "user");
        assert_eq!(converted[1]["role"], "assistant");
    }

    #[test]
    fn test_convert_messages_no_system() {
        let messages = vec![LlmMessage { role: "user".into(), content: "Hello".into() }];

        let (system, converted) = convert_messages(messages);
        assert_eq!(system, None);
        assert_eq!(converted.len(), 1);
    }
}
