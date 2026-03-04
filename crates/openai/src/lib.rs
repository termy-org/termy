use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_MODEL: &str = "gpt-5-mini";

#[derive(Error, Debug)]
pub enum OpenAiError {
    #[error("HTTP request failed: {0}")]
    RequestFailed(Box<ureq::Error>),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("API error: {message}")]
    ApiError { message: String },

    #[error("No response content")]
    NoContent,

    #[error("API key not configured")]
    ApiKeyMissing,
}

impl From<ureq::Error> for OpenAiError {
    fn from(error: ureq::Error) -> Self {
        Self::RequestFailed(Box::new(error))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: ChatContent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ChatContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { image_url: ImageUrl },
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageUrl {
    pub url: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: ChatContent::Text(content.into()),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: ChatContent::Text(content.into()),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: ChatContent::Text(content.into()),
        }
    }

    pub fn user_with_file(text: impl Into<String>, file_content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: ChatContent::Parts(vec![ContentPart::Text {
                text: format!(
                    "{}\n\nTerminal context:\n```\n{}\n```",
                    text.into(),
                    file_content.into()
                ),
            }]),
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Option<Vec<Choice>>,
    #[serde(default)]
    error: Option<ApiErrorResponse>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelResponse>,
    #[serde(default)]
    error: Option<ApiErrorResponse>,
}

#[derive(Debug, Deserialize)]
struct ModelResponse {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiModel {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct OpenAiClient {
    api_key: String,
    model: String,
}

impl OpenAiClient {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: DEFAULT_MODEL.to_string(),
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Blocking chat API call
    pub fn chat(&self, messages: Vec<ChatMessage>) -> Result<String, OpenAiError> {
        self.chat_with_options(messages, None, None)
    }

    /// Blocking chat API call with options
    pub fn chat_with_options(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, OpenAiError> {
        let request = ChatRequest {
            model: self.model.clone(),
            messages,
            max_tokens,
            temperature,
        };

        let response = ureq::post("https://api.openai.com/v1/chat/completions")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(&request)?;

        let chat_response: ChatResponse = response.into_json()?;

        if let Some(error) = chat_response.error {
            return Err(OpenAiError::ApiError {
                message: error.message,
            });
        }

        chat_response
            .choices
            .and_then(|choices| choices.into_iter().next())
            .and_then(|choice| choice.message.content)
            .ok_or(OpenAiError::NoContent)
    }

    /// Simple message without any context
    pub fn simple_message(&self, user_message: impl Into<String>) -> Result<String, OpenAiError> {
        self.chat(vec![ChatMessage::user(user_message)])
    }

    /// Message with terminal context
    pub fn message_with_terminal_context(
        &self,
        user_message: impl Into<String>,
        terminal_content: impl Into<String>,
    ) -> Result<String, OpenAiError> {
        let system = ChatMessage::system(
            "You are a helpful terminal assistant. The user will provide terminal context \
             (recent commands and output). Help them with their question. When suggesting \
             commands, be concise and provide only the command they should run. \
             If they ask for a command, respond with just the command, no explanation unless asked.",
        );
        let user = ChatMessage::user_with_file(user_message, terminal_content);

        self.chat(vec![system, user])
    }

    /// Fetch all available models for this API key.
    pub fn fetch_models(&self) -> Result<Vec<OpenAiModel>, OpenAiError> {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(5))
            .timeout_read(std::time::Duration::from_secs(10))
            .timeout_write(std::time::Duration::from_secs(10))
            .build();
        let response = agent
            .get("https://api.openai.com/v1/models")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .call()?;

        let models_response: ModelsResponse = response.into_json()?;
        if let Some(error) = models_response.error {
            return Err(OpenAiError::ApiError {
                message: error.message,
            });
        }

        let mut models: Vec<OpenAiModel> = models_response
            .data
            .into_iter()
            .map(|model| OpenAiModel { id: model.id })
            .collect();
        models.sort_by(|left, right| left.id.cmp(&right.id));
        models.dedup_by(|left, right| left.id == right.id);
        Ok(models)
    }

    /// Fetch models likely usable for chat-style prompts.
    pub fn fetch_chat_models(&self) -> Result<Vec<OpenAiModel>, OpenAiError> {
        let models = self.fetch_models()?;
        Ok(models
            .into_iter()
            .filter(|model| {
                model.id.starts_with("gpt-")
                    || (model.id.starts_with('o')
                        && model
                            .id
                            .chars()
                            .nth(1)
                            .map(|ch| ch.is_ascii_digit())
                            .unwrap_or(false))
                    || model.id.starts_with("chatgpt-")
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_message_creation() {
        let user_msg = ChatMessage::user("Hello");
        assert_eq!(user_msg.role, "user");

        let system_msg = ChatMessage::system("You are a helpful assistant");
        assert_eq!(system_msg.role, "system");

        let assistant_msg = ChatMessage::assistant("How can I help?");
        assert_eq!(assistant_msg.role, "assistant");
    }

    #[test]
    fn test_client_creation() {
        let client = OpenAiClient::new("test-key");
        assert_eq!(client.model, DEFAULT_MODEL);

        let client = client.with_model("gpt-4");
        assert_eq!(client.model, "gpt-4");
    }
}
