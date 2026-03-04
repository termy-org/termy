use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

#[derive(Error, Debug)]
pub enum GeminiError {
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
}

impl From<ureq::Error> for GeminiError {
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
pub struct GeminiModel {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct GeminiClient {
    api_key: String,
    model: String,
}

impl GeminiClient {
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

    fn normalized_model_id(&self) -> String {
        self.model.trim().to_string()
    }

    pub fn message_with_terminal_context(
        &self,
        user_message: impl Into<String>,
        terminal_content: impl Into<String>,
    ) -> Result<String, GeminiError> {
        let system = ChatMessage::system(
            "You are a helpful terminal assistant. The user will provide terminal context \
             (recent commands and output). Help them with their question. When suggesting \
             commands, be concise and provide only the command they should run. \
             If they ask for a command, respond with just the command, no explanation unless asked.",
        );
        let user = ChatMessage::user_with_file(user_message, terminal_content);

        let request = ChatRequest {
            model: self.normalized_model_id(),
            messages: vec![system, user],
            max_tokens: None,
            temperature: None,
        };

        let agent = ureq::AgentBuilder::new()
            .timeout_read(std::time::Duration::from_secs(10))
            .timeout_write(std::time::Duration::from_secs(10))
            .build();
        let response = agent
            .post("https://generativelanguage.googleapis.com/v1beta/openai/chat/completions")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_json(&request)?;
        let chat_response: ChatResponse = response.into_json()?;

        if let Some(error) = chat_response.error {
            return Err(GeminiError::ApiError {
                message: error.message,
            });
        }

        chat_response
            .choices
            .and_then(|choices| choices.into_iter().next())
            .and_then(|choice| choice.message.content)
            .ok_or(GeminiError::NoContent)
    }

    pub fn fetch_models(&self) -> Result<Vec<GeminiModel>, GeminiError> {
        let response = ureq::get("https://generativelanguage.googleapis.com/v1beta/openai/models")
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .call()?;

        let models_response: ModelsResponse = response.into_json()?;
        if let Some(error) = models_response.error {
            return Err(GeminiError::ApiError {
                message: error.message,
            });
        }

        let mut models: Vec<GeminiModel> = models_response
            .data
            .into_iter()
            .map(|model| GeminiModel {
                id: model.id.trim_start_matches("models/").to_string(),
            })
            .collect();
        models.sort_by(|left, right| left.id.cmp(&right.id));
        models.dedup_by(|left, right| left.id == right.id);
        Ok(models)
    }

    pub fn fetch_chat_models(&self) -> Result<Vec<GeminiModel>, GeminiError> {
        let models = self.fetch_models()?;
        Ok(models
            .into_iter()
            .filter(|model| model.id.starts_with("gemini-"))
            .collect())
    }
}
