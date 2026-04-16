use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProviderType {
    OpenRouter,
    Groq,
    Cerebras,
    Cohere,
    Pollinations,
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderType::OpenRouter => write!(f, "OpenRouter"),
            ProviderType::Groq => write!(f, "Groq"),
            ProviderType::Cerebras => write!(f, "Cerebras"),
            ProviderType::Cohere => write!(f, "Cohere"),
            ProviderType::Pollinations => write!(f, "Pollinations"),
        }
    }
}

impl ProviderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::OpenRouter => "openrouter",
            ProviderType::Groq => "groq",
            ProviderType::Cerebras => "cerebras",
            ProviderType::Cohere => "cohere",
            ProviderType::Pollinations => "pollinations",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "openrouter" => Some(ProviderType::OpenRouter),
            "groq" => Some(ProviderType::Groq),
            "cerebras" => Some(ProviderType::Cerebras),
            "cohere" => Some(ProviderType::Cohere),
            "pollinations" => Some(ProviderType::Pollinations),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider: ProviderType,
    pub context_length: Option<i64>,
    pub is_free: bool,
    pub description: Option<String>,
    pub supports_tools: bool,
    pub supports_reasoning: bool,
    pub supports_native_search: bool,
    pub is_specialized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub prompt: String,
    pub completion: String,
}

// Internal structures for API responses
#[derive(Deserialize)]
struct OpenRouterModel {
    id: String,
    name: String,
    pricing: Option<OpenRouterPricing>,
    context_length: Option<i64>,
}

#[derive(Deserialize)]
struct OpenRouterPricing {
    prompt: String,
    completion: String,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    data: Vec<OpenRouterModel>,
}

#[derive(Deserialize)]
struct StandardModelResponse {
    data: Vec<StandardModel>,
}

#[derive(Deserialize)]
struct StandardModel {
    id: String,
    context_window: Option<i64>, // For Groq
}

#[derive(Deserialize)]
struct CerebrasModelResponse {
    data: Vec<CerebrasModel>,
}

#[derive(Deserialize)]
struct CerebrasModel {
    id: String,
    limits: Option<CerebrasLimits>,
}

#[derive(Deserialize)]
struct CerebrasLimits {
    max_context_length: Option<i64>,
}

#[derive(Deserialize)]
struct CohereModelResponse {
    models: Vec<CohereModel>,
}

#[derive(Deserialize)]
struct CohereModel {
    name: String,
    context_length: Option<i64>,
}

#[derive(Deserialize)]
struct PollinationsModel {
    name: String,
    aliases: Option<Vec<String>>,
    description: Option<String>,
    #[serde(rename = "context_window")]
    context_window: Option<i64>,
    tools: Option<bool>,
    reasoning: Option<bool>,
    is_specialized: Option<bool>,
}

const POLLINATIONS_ALLOWED_MODELS: &[&str] = &[
    "nova-fast",
    "gemini-search",
    "perplexity-fast",
    "qwen-safety",
];

const SEARCH_PRIORITY: &[&str] = &[
    "perplexity-fast",
    "gemini-search",
    "sonar",
    "search",
];

const GENERAL_PRIORITY: &[&str] = &[
    "deepseek-r1",
    "claude",
    "gpt-4",
    "qwen",
    "gemini",
    "nova-fast",
    "perplexity-fast",
];

fn infer_native_search(id: &str, name: &str, description: Option<&str>) -> bool {
    let haystack = format!(
        "{} {} {}",
        id.to_lowercase(),
        name.to_lowercase(),
        description.unwrap_or_default().to_lowercase()
    );

    haystack.contains("search") || haystack.contains("sonar") || haystack.contains("web search")
}

fn infer_reasoning(id: &str, name: &str, description: Option<&str>) -> bool {
    let haystack = format!(
        "{} {} {}",
        id.to_lowercase(),
        name.to_lowercase(),
        description.unwrap_or_default().to_lowercase()
    );

    haystack.contains("reasoning") || haystack.contains("think") || haystack.contains("r1")
}

fn infer_specialized(description: Option<&str>, is_specialized: bool) -> bool {
    is_specialized
        || description
            .unwrap_or_default()
            .to_lowercase()
            .contains("moderation")
}

pub struct LLMManager {
    db: Arc<crate::db::Database>,
    models: Arc<RwLock<Vec<Model>>>,
    api_keys: HashMap<ProviderType, String>,
}

impl LLMManager {
    pub fn new(db: Arc<crate::db::Database>) -> Self {
        let mut api_keys = HashMap::new();
        
        if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
            api_keys.insert(ProviderType::OpenRouter, key);
        }
        if let Ok(key) = std::env::var("GROQ_API_KEY") {
            api_keys.insert(ProviderType::Groq, key);
        }
        if let Ok(key) = std::env::var("CEREBRAS_API_KEY") {
            api_keys.insert(ProviderType::Cerebras, key);
        }
        if let Ok(key) = std::env::var("COHERE_API_KEY") {
            api_keys.insert(ProviderType::Cohere, key);
        }
        if let Ok(key) = std::env::var("POLLINATIONS_API_KEY") {
            api_keys.insert(ProviderType::Pollinations, key);
        }

        Self {
            db,
            models: Arc::new(RwLock::new(Vec::new())),
            api_keys,
        }
    }

    pub async fn fetch_available_models(&self) -> Result<()> {
        let mut all_models = Vec::new();
        // Use a client with timeout to prevent hanging during startup
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        // 1. OpenRouter (Free models)
        if let Some(key) = self.api_keys.get(&ProviderType::OpenRouter) {
            tracing::info!("Fetching OpenRouter models...");
            match self.fetch_openrouter_models(&client, key).await {
                Ok(mut models) => all_models.append(&mut models),
                Err(e) => tracing::error!("Failed to fetch OpenRouter models: {}", e),
            }
            
            // Also fetch OpenRouter limits
            if let Err(e) = self.fetch_openrouter_limits(&client, key).await {
                tracing::warn!("Failed to fetch OpenRouter limits: {}", e);
            }
        }

        // 2. Groq
        if let Some(key) = self.api_keys.get(&ProviderType::Groq) {
            tracing::info!("Fetching Groq models...");
            match self.fetch_groq_models(&client, key).await {
                Ok(mut models) => all_models.append(&mut models),
                Err(e) => tracing::error!("Failed to fetch Groq models: {}", e),
            }
        }

        // 3. Cerebras
        if let Some(key) = self.api_keys.get(&ProviderType::Cerebras) {
            tracing::info!("Fetching Cerebras models...");
            match self.fetch_cerebras_models(&client, key).await {
                Ok(mut models) => all_models.append(&mut models),
                Err(e) => tracing::error!("Failed to fetch Cerebras models: {}", e),
            }
        }

        // 4. Cohere
        if let Some(key) = self.api_keys.get(&ProviderType::Cohere) {
            tracing::info!("Fetching Cohere models...");
            match self.fetch_cohere_models(&client, key).await {
                Ok(mut models) => all_models.append(&mut models),
                Err(e) => tracing::error!("Failed to fetch Cohere models: {}", e),
            }
        }

        // 5. Pollinations
        tracing::info!("Fetching Pollinations models...");
        match self.fetch_pollinations_models(&client).await {
            Ok(mut models) => all_models.append(&mut models),
            Err(e) => tracing::error!("Failed to fetch Pollinations models: {}", e),
        }

        if let Some(key) = self.api_keys.get(&ProviderType::Pollinations) {
            if let Err(e) = self.fetch_pollinations_limits(&client, key).await {
                tracing::warn!("Failed to fetch Pollinations limits: {}", e);
            }
        }

        let count = all_models.len();
        {
            let mut w = self.models.write().await;
            *w = all_models;
        }
        tracing::info!("Successfully updated model list. Total models: {}", count);
        
        Ok(())
    }
    
    pub async fn refresh_llm_limits(&self) -> Result<()> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;

        if let Some(key) = self.api_keys.get(&ProviderType::OpenRouter) {
            let _ = self.fetch_openrouter_limits(&client, key).await;
        }

        if let Some(key) = self.api_keys.get(&ProviderType::Pollinations) {
            let _ = self.fetch_pollinations_limits(&client, key).await;
        }

        Ok(())
    }

    async fn fetch_openrouter_limits(&self, client: &reqwest::Client, key: &str) -> Result<()> {
        let resp = client.get("https://openrouter.ai/api/v1/key")
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await?;
            
        if resp.status().is_success() {
            let json: serde_json::Value = resp.json().await?;
            if let Some(data) = json.get("data") {
                let requests_limit = data.get("rate_limit")
                    .and_then(|rl| rl.get("requests"))
                    .and_then(|r| r.as_i64());
                    
                let interval = data.get("rate_limit")
                    .and_then(|rl| rl.get("interval"))
                    .and_then(|i| i.as_str());

                // Map to day/min limits
                let mut limit_day = None;
                
                if let Some(reqs) = requests_limit {
                    if interval == Some("1d") {
                        limit_day = Some(reqs);
                    }
                }
                
                // Update DB if we found a limit
                if limit_day.is_some() {
                    self.db.update_provider_limits(
                        &ProviderType::OpenRouter,
                        None,
                        None,
                        None,
                        limit_day
                    ).await?;
                }
            }
        }
        Ok(())
    }

    async fn fetch_openrouter_models(&self, client: &reqwest::Client, _key: &str) -> Result<Vec<Model>> {
        let resp: OpenRouterResponse = client.get("https://openrouter.ai/api/v1/models")
            .send()
            .await?
            .json()
            .await?;

        // Parse allowed models from env
        let allowed_models: Vec<String> = std::env::var("OPENROUTER_MODELS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let models = resp.data.into_iter()
            .filter(|m| {
                let p = m.pricing.as_ref();
                let is_free = if let Some(pricing) = p {
                    let prompt_free = pricing.prompt.parse::<f64>().unwrap_or(1.0) <= 0.000001;
                    let completion_free = pricing.completion.parse::<f64>().unwrap_or(1.0) <= 0.000001;
                    prompt_free && completion_free
                } else {
                    false
                };

                if !allowed_models.is_empty() {
                    allowed_models.contains(&m.id)
                } else {
                    is_free
                }
            })
            .map(|m| {
                let id = m.id;
                let name = m.name;
                let supports_reasoning = infer_reasoning(&id, &name, None);
                let supports_native_search = infer_native_search(&id, &name, None);

                Model {
                    id,
                    name,
                    provider: ProviderType::OpenRouter,
                    context_length: m.context_length,
                    is_free: true,
                    description: None,
                    supports_tools: true,
                    supports_reasoning,
                    supports_native_search,
                    is_specialized: false,
                }
            })
            .collect();
        
        Ok(models)
    }

    async fn fetch_groq_models(&self, client: &reqwest::Client, key: &str) -> Result<Vec<Model>> {
        let resp: StandardModelResponse = client.get("https://api.groq.com/openai/v1/models")
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await?
            .json()
            .await?;

        let models = resp.data.into_iter()
            .map(|m| {
                let id = m.id;
                let name = id.clone();
                let supports_reasoning = infer_reasoning(&id, &name, None);
                let supports_native_search = infer_native_search(&id, &name, None);

                Model {
                    id,
                    name,
                    provider: ProviderType::Groq,
                    context_length: m.context_window,
                    is_free: false,
                    description: None,
                    supports_tools: true,
                    supports_reasoning,
                    supports_native_search,
                    is_specialized: false,
                }
            })
            .collect();
        
        Ok(models)
    }

    async fn fetch_cerebras_models(&self, client: &reqwest::Client, _key: &str) -> Result<Vec<Model>> {
        // Use public endpoint for better metadata
        let resp: CerebrasModelResponse = client.get("https://api.cerebras.ai/public/v1/models")
            .send()
            .await?
            .json()
            .await?;

        let models = resp.data.into_iter()
            .map(|m| {
                let id = m.id;
                let name = id.clone();
                let supports_reasoning = infer_reasoning(&id, &name, None);
                let supports_native_search = infer_native_search(&id, &name, None);

                Model {
                    id,
                    name,
                    provider: ProviderType::Cerebras,
                    context_length: m.limits.and_then(|l| l.max_context_length),
                    is_free: false,
                    description: None,
                    supports_tools: true,
                    supports_reasoning,
                    supports_native_search,
                    is_specialized: false,
                }
            })
            .collect();
        
        Ok(models)
    }

    async fn fetch_cohere_models(&self, client: &reqwest::Client, key: &str) -> Result<Vec<Model>> {
        let resp: CohereModelResponse = client.get("https://api.cohere.ai/v1/models")
            .header("Authorization", format!("Bearer {}", key))
            .header("X-Client-Name", "w9-search")
            .send()
            .await?
            .json()
            .await?;

        let models = resp.models.into_iter()
            .map(|m| {
                let id = m.name;
                let name = id.clone();
                let supports_reasoning = infer_reasoning(&id, &name, None);
                let supports_native_search = infer_native_search(&id, &name, None);

                Model {
                    id,
                    name,
                    provider: ProviderType::Cohere,
                    context_length: m.context_length.map(|l| l as i64),
                    is_free: false,
                    description: None,
                    supports_tools: false,
                    supports_reasoning,
                    supports_native_search,
                    is_specialized: false,
                }
            })
            .collect();
        
        Ok(models)
    }

    async fn fetch_pollinations_models(&self, client: &reqwest::Client) -> Result<Vec<Model>> {
        // Fetch from gen.pollinations.ai/text/models for metadata
        let resp: Vec<PollinationsModel> = client.get("https://gen.pollinations.ai/text/models")
            .send()
            .await?
            .json()
            .await?;

        let allowed: HashSet<&'static str> = POLLINATIONS_ALLOWED_MODELS.iter().copied().collect();

        let models = resp.into_iter()
            .filter(|m| {
                allowed.contains(m.name.as_str())
                    || m.aliases
                        .as_ref()
                        .map(|aliases| aliases.iter().any(|alias| allowed.contains(alias.as_str())))
                        .unwrap_or(false)
            })
            .map(|m| {
                let id = m.name;
                let name = id.clone();
                let description = m.description.clone();
                let description_lc = description.as_deref().unwrap_or_default().to_lowercase();
                let search_hint = infer_native_search(&id, &name, description.as_deref())
                    || m.aliases
                        .as_ref()
                        .map(|aliases| aliases.iter().any(|alias| alias.to_lowercase().contains("search") || alias.to_lowercase().contains("sonar")))
                        .unwrap_or(false);
                let supports_reasoning = m.reasoning.unwrap_or(false)
                    || description_lc.contains("reasoning")
                    || infer_reasoning(&id, &name, description.as_deref());
                let is_specialized = infer_specialized(description.as_deref(), m.is_specialized.unwrap_or(false));

                Model {
                    id,
                    name,
                    provider: ProviderType::Pollinations,
                    context_length: m.context_window.or(Some(16000)),
                    is_free: true,
                    description,
                    supports_tools: m.tools.unwrap_or(false),
                    supports_reasoning,
                    supports_native_search: search_hint,
                    is_specialized,
                }
            })
            .collect();
        
        Ok(models)
    }

    async fn fetch_pollinations_limits(&self, client: &reqwest::Client, key: &str) -> Result<()> {
        let resp = client.get("https://gen.pollinations.ai/account/balance")
            .header("Authorization", format!("Bearer {}", key))
            .send()
            .await?;
            
        if resp.status().is_success() {
            let json: serde_json::Value = resp.json().await?;
            if let Some(balance) = json.get("balance").and_then(|b| b.as_f64()) {
                // Map pollen balance to "day" limits for the dashboard bar
                // We'll treat 1000 pollen as a reference "limit" if no budget is set, 
                // or just store the balance as remaining.
                self.db.update_provider_limits(
                    &ProviderType::Pollinations,
                    None,
                    None,
                    None, // We'll use the "day" field to store balance/remaining
                    Some(balance as i64) 
                ).await?;
            }
        }
        Ok(())
    }

    pub async fn get_models(&self) -> Vec<Model> {
        self.models.read().await.clone()
    }

    pub async fn get_model(&self, id: &str) -> Option<Model> {
        self.models.read().await.iter().find(|m| m.id == id).cloned()
    }

    pub async fn pick_default_model(&self, prefer_search: bool) -> Option<Model> {
        let models = self.get_models().await;
        if models.is_empty() {
            return None;
        }

        let mut candidates: Vec<Model> = models
            .into_iter()
            .filter(|m| !m.is_specialized)
            .collect();

        if candidates.is_empty() {
            candidates = self.get_models().await;
        }

        let priority_list = if prefer_search {
            SEARCH_PRIORITY
        } else {
            GENERAL_PRIORITY
        };

        for needle in priority_list {
            if let Some(model) = candidates.iter().find(|m| {
                let id = m.id.to_lowercase();
                let name = m.name.to_lowercase();
                let description = m.description.as_deref().unwrap_or_default().to_lowercase();
                id.contains(needle) || name.contains(needle) || description.contains(needle)
            }) {
                return Some(model.clone());
            }
        }

        if prefer_search {
            if let Some(model) = candidates.iter().find(|m| m.supports_native_search) {
                return Some(model.clone());
            }
        }

        candidates
            .iter()
            .find(|m| m.is_free)
            .cloned()
            .or_else(|| candidates.first().cloned())
    }
    
    pub async fn check_rate_limit(&self, provider: ProviderType) -> Result<bool> {
        self.db.check_rate_limit(&provider).await
    }

    pub async fn chat_completion(
        &self,
        model_id: &str,
        messages: Vec<serde_json::Value>,
        tools: Option<Vec<serde_json::Value>>,
        reasoning_enabled: bool,
    ) -> Result<serde_json::Value> {
        let model = self.get_model(model_id).await
            .ok_or_else(|| anyhow::anyhow!("Model {} not found", model_id))?;
        
        let provider = model.provider;
        
        if !self.check_rate_limit(provider.clone()).await? {
            return Err(anyhow::anyhow!("Rate limit exceeded for provider {}", provider));
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()?;

        let api_key = self.api_keys.get(&provider).cloned();

        match provider {
            ProviderType::OpenRouter => {
                let key = api_key
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("API key not found for provider {}", provider))?;
                let request = serde_json::json!({
                    "model": model_id,
                    "messages": messages,
                    "tools": tools
                });
                
                let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
                let resp = client.post("https://openrouter.ai/api/v1/chat/completions")
                    .header("Authorization", format!("Bearer {}", key))
                    .header("Content-Type", "application/json")
                    .header("HTTP-Referer", format!("http://localhost:{}", port))
                    .header("X-Title", "W9 Search")
                    .json(&request)
                    .send()
                    .await?;
                    
                if !resp.status().is_success() {
                    let text = resp.text().await?;
                    return Err(anyhow::anyhow!("OpenRouter Error: {}", text));
                }
                
                Ok(resp.json().await?)
            },
            ProviderType::Groq => {
                let key = api_key
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("API key not found for provider {}", provider))?;
                let request = serde_json::json!({
                    "model": model_id,
                    "messages": messages,
                    "tools": tools
                });
                
                let resp = client.post("https://api.groq.com/openai/v1/chat/completions")
                    .header("Authorization", format!("Bearer {}", key))
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    let text = resp.text().await?;
                    return Err(anyhow::anyhow!("Groq Error: {}", text));
                }
                
                let headers = resp.headers();
                let remaining_req = headers.get("x-ratelimit-remaining-requests")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<i64>().ok());
                let limit_req = headers.get("x-ratelimit-limit-requests")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<i64>().ok());
                    
                if remaining_req.is_some() || limit_req.is_some() {
                    let _ = self.db.update_provider_limits(&ProviderType::Groq, None, remaining_req, None, limit_req).await;
                }

                Ok(resp.json().await?)
            },
            ProviderType::Cerebras => {
                let key = api_key
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("API key not found for provider {}", provider))?;
                let request = serde_json::json!({
                    "model": model_id,
                    "messages": messages,
                    "tools": tools
                });
                
                let resp = client.post("https://api.cerebras.ai/v1/chat/completions")
                    .header("Authorization", format!("Bearer {}", key))
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    let text = resp.text().await?;
                    return Err(anyhow::anyhow!("Cerebras Error: {}", text));
                }

                let headers = resp.headers();
                let remaining_day = headers.get("x-ratelimit-remaining-requests-day")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<i64>().ok());
                let limit_day = headers.get("x-ratelimit-limit-requests-day")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<i64>().ok());
                    
                if remaining_day.is_some() || limit_day.is_some() {
                    let _ = self.db.update_provider_limits(&ProviderType::Cerebras, None, remaining_day, None, limit_day).await;
                }

                Ok(resp.json().await?)
            },
            ProviderType::Cohere => {
                let key = api_key
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("API key not found for provider {}", provider))?;
                let last_message = messages.last()
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| anyhow::anyhow!("No content in last message"))?;

                let mut chat_history = Vec::new();
                for msg in messages.iter().take(messages.len() - 1) {
                    if let (Some(role), Some(content)) = (msg.get("role").and_then(|r| r.as_str()), msg.get("content").and_then(|c| c.as_str())) {
                        let cohere_role = match role {
                            "user" => "USER",
                            "assistant" => "CHATBOT",
                            "system" => "SYSTEM",
                            _ => "USER",
                        };
                        chat_history.push(serde_json::json!({
                            "role": cohere_role,
                            "message": content
                        }));
                    }
                }

                let request = serde_json::json!({
                    "model": model_id,
                    "message": last_message,
                    "chat_history": chat_history,
                });

                let resp = client.post("https://api.cohere.ai/v1/chat")
                    .header("Authorization", format!("Bearer {}", key))
                    .header("Content-Type", "application/json")
                    .header("X-Client-Name", "w9-search")
                    .json(&request)
                    .send()
                    .await?;

                if !resp.status().is_success() {
                    let text = resp.text().await?;
                    return Err(anyhow::anyhow!("Cohere Error: {}", text));
                }

                let cohere_resp: serde_json::Value = resp.json().await?;
                
                let text_response = cohere_resp.get("text").and_then(|t| t.as_str()).unwrap_or("");
                
                Ok(serde_json::json!({
                    "id": cohere_resp.get("generation_id"),
                    "object": "chat.completion",
                    "created": chrono::Utc::now().timestamp(),
                    "model": model_id,
                    "choices": [
                        {
                            "index": 0,
                            "message": {
                                "role": "assistant",
                                "content": text_response
                            },
                            "finish_reason": "stop"
                        }
                    ],
                    "usage": {
                        "prompt_tokens": 0,
                        "completion_tokens": 0,
                        "total_tokens": 0
                    }
                }))
            },
            ProviderType::Pollinations => {
                let mut request = serde_json::json!({
                    "model": model_id,
                    "messages": messages,
                });

                if let Some(tools) = tools.filter(|_| model.supports_tools) {
                    request["tools"] = serde_json::Value::Array(tools);
                }

                if reasoning_enabled && model.supports_reasoning {
                    request["reasoning"] = serde_json::json!({
                        "type": "enabled",
                        "budget_tokens": 2048
                    });
                    request["reasoning_effort"] = serde_json::json!("medium");
                }

                let mut req = client.post("https://gen.pollinations.ai/v1/chat/completions")
                    .header("Content-Type", "application/json")
                    .json(&request);

                if let Some(key) = api_key.as_ref() {
                    req = req.header("Authorization", format!("Bearer {}", key));
                }

                let resp = req.send().await?;

                if !resp.status().is_success() {
                    let text = resp.text().await?;
                    return Err(anyhow::anyhow!("Pollinations Error: {}", text));
                }

                Ok(resp.json().await?)
            }
        }
    }
}
