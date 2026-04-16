use crate::db::Database;
use crate::llm::LLMManager;
use crate::search::WebSearch;
use crate::tools::Tools;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

pub struct RAGSystem {
    db: Arc<Database>,
    llm_manager: Arc<LLMManager>,
    model: crate::llm::Model,
    search_provider: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamEvent {
    Status(String),
    Source(crate::models::Source),
    Answer(String),
    Error(String),
    Done,
}

impl RAGSystem {
    pub fn new(
        db: Arc<Database>,
        llm_manager: Arc<LLMManager>,
        model: crate::llm::Model,
        search_provider: Option<String>,
    ) -> Self {
        Self {
            db,
            llm_manager,
            model,
            search_provider,
        }
    }

    async fn send_status(
        &self,
        sender: &Option<Sender<Result<StreamEvent, anyhow::Error>>>,
        message: impl Into<String>,
    ) {
        if let Some(tx) = sender {
            let _ = tx.send(Ok(StreamEvent::Status(message.into()))).await;
        }
    }

    /// Ask the LLM to plan the research steps
    async fn plan_search(&self, query: &str, reasoning_enabled: bool) -> Result<Vec<String>> {
        tracing::info!("Planning search for query: {}", query);

        let system_prompt = if reasoning_enabled {
            "You are a thorough research planner. Given a user query, generate 2-5 complementary search queries that would help answer it well. \
            Include direct factual queries, recent-context queries, and one query that checks for counterexamples or edge cases when useful. \
            Return ONLY a JSON object with a 'queries' key containing the list of strings. \
            Example: {\"queries\": [\"current president of US\", \"US president term length\"]}"
        } else {
            "You are a simplified research planner. Given a user query, generate a list of 1-3 specific search queries that would help answer it. \
            Return ONLY a JSON object with a 'queries' key containing the list of strings. \
            Example: {\"queries\": [\"current president of US\", \"US president term length\"]}"
        };

        let messages = vec![
            json!({ "role": "system", "content": system_prompt }),
            json!({ "role": "user", "content": query }),
        ];

        let json_resp = self
            .llm_manager
            .chat_completion(
                &self.model.id,
                messages,
                None,
                reasoning_enabled && self.model.supports_reasoning,
            )
            .await?;

        // Extract content from choice
        let content = json_resp["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("{}");

        // Clean markdown code blocks if present
        let clean_content = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```");

        if let Ok(plan) = serde_json::from_str::<Value>(clean_content) {
            if let Some(queries) = plan["queries"].as_array() {
                let strings: Vec<String> = queries
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                tracing::info!("Generated search plan: {:?}", strings);
                return Ok(strings);
            }
        }

        // Fallback: just use the original query
        Ok(vec![query.to_string()])
    }

    /// Enhance search query with temporal context for time-sensitive queries
    fn enhance_query_with_temporal_context(query: &str) -> String {
        let query_lower = query.to_lowercase();

        // Check if query is time-sensitive
        let time_sensitive_keywords = [
            "current",
            "today",
            "now",
            "present",
            "latest",
            "recent",
            "who is",
            "what is the current",
            "who are the current",
            "president",
            "leader",
            "ceo",
            "chairman",
            "minister",
            "happened today",
            "news",
            "breaking",
            "update",
        ];

        // Check for comparison queries that might need calculation
        let needs_calculation = query_lower.contains("compare")
            || query_lower.contains("difference")
            || query_lower.contains("larger")
            || query_lower.contains("smaller")
            || query_lower.contains("more than")
            || query_lower.contains("less than");

        // Check for unit conversion queries
        let needs_conversion = query_lower.contains("convert")
            || query_lower.contains("to ")
                && (query_lower.contains("km")
                    || query_lower.contains("miles")
                    || query_lower.contains("celsius")
                    || query_lower.contains("fahrenheit")
                    || query_lower.contains("kg")
                    || query_lower.contains("pounds"));

        let is_time_sensitive = time_sensitive_keywords
            .iter()
            .any(|keyword| query_lower.contains(keyword));

        if is_time_sensitive {
            // Get current date to add context
            let current_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
            format!("{} as of {}", query, current_date)
        } else if needs_calculation {
            // Add context for calculation queries
            format!("{} (use calculation tools if needed)", query)
        } else if needs_conversion {
            // Add context for conversion queries
            format!("{} (use unit conversion tools if needed)", query)
        } else {
            query.to_string()
        }
    }

    pub async fn query(
        &self,
        user_query: &str,
        web_search_enabled: bool,
        search_reasoning_enabled: bool,
        history: Vec<crate::models::Message>,
        status_sender: Option<Sender<Result<StreamEvent, anyhow::Error>>>,
    ) -> Result<(String, Vec<crate::models::Source>)> {
        tracing::info!(
            "Starting RAG query: '{}' (web_search: {}, reasoning: {}, history: {})",
            user_query,
            web_search_enabled,
            search_reasoning_enabled,
            history.len()
        );
        self.send_status(&status_sender, "Initializing search...")
            .await;

        let mut context_sources = Vec::new();
        let native_search = web_search_enabled && self.model.supports_native_search;
        let agentic_search = web_search_enabled && !native_search;

        // Step 1: External web search when the chosen model cannot search on its own.
        if agentic_search {
            self.send_status(&status_sender, "Planning research strategy...")
                .await;

            let search_queries = match self.plan_search(user_query, search_reasoning_enabled).await
            {
                Ok(queries) => queries,
                Err(e) => {
                    tracing::warn!("Planning failed: {}, falling back to single query", e);
                    vec![Self::enhance_query_with_temporal_context(user_query)]
                }
            };

            self.send_status(
                &status_sender,
                format!("Identified {} search queries", search_queries.len()),
            )
            .await;

            let mut all_results = Vec::new();
            let mut seen_urls = HashSet::new();

            for query in search_queries {
                self.send_status(&status_sender, format!("Searching: {}", query))
                    .await;
                tracing::info!("Executing search step: {}", query);
                if let Ok(results) =
                    WebSearch::search(&self.db, &query, self.search_provider.as_deref()).await
                {
                    for result in results {
                        if seen_urls.insert(result.url.clone()) {
                            all_results.push((query.clone(), result));
                        }
                    }
                }
            }

            let fetch_limit = if search_reasoning_enabled { 7 } else { 5 };
            self.send_status(
                &status_sender,
                format!(
                    "Found {} potential sources. Reading content...",
                    all_results.len()
                ),
            )
            .await;

            for (idx, (query_hint, result)) in all_results.iter().take(fetch_limit).enumerate() {
                self.send_status(&status_sender, format!("Reading: {}", result.title))
                    .await;
                tracing::info!("Fetching content from result {}: {}", idx + 1, result.url);

                let content = match WebSearch::fetch_content(
                    &result.url,
                    Some(query_hint.as_str()),
                    Some(result.snippet.as_str()),
                    Some(result.title.as_str()),
                )
                .await
                {
                    Ok(content) => content,
                    Err(e) => {
                        tracing::warn!(
                            "Failed to fetch {}: {}. Falling back to snippet.",
                            result.url,
                            e
                        );
                        if result.snippet.trim().is_empty() {
                            continue;
                        }
                        result.snippet.clone()
                    }
                };

                tracing::info!("Fetched {} bytes from {}", content.len(), result.url);
                match self
                    .db
                    .insert_source(&result.url, &result.title, &content)
                    .await
                {
                    Ok(id) => {
                        tracing::info!("Stored source {} in database", id);
                        let source = crate::models::Source {
                            id,
                            url: result.url.clone(),
                            title: result.title.clone(),
                            content,
                            created_at: chrono::Utc::now(),
                        };

                        if let Some(tx) = &status_sender {
                            let _ = tx.send(Ok(StreamEvent::Source(source.clone()))).await;
                        }

                        context_sources.push(source);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to store source {}: {}", result.url, e);
                    }
                }
            }
        } else if native_search {
            self.send_status(
                &status_sender,
                "Using a native-search model; skipping local web search.",
            )
            .await;
        }

        // Step 2: Retrieve relevant sources from database (always check DB too)
        self.send_status(&status_sender, "Checking internal knowledge base...")
            .await;
        tracing::info!("Searching database for relevant sources...");
        let db_sources = match self
            .db
            .search_sources(user_query, if search_reasoning_enabled { 5 } else { 3 })
            .await
        {
            Ok(sources) => {
                tracing::info!("Found {} relevant sources in database", sources.len());
                sources
            }
            Err(e) => {
                tracing::warn!(
                    "Database search failed: {}, continuing without DB sources",
                    e
                );
                Vec::new()
            }
        };

        // Merge and deduplicate
        let mut seen_ids = HashSet::new();
        for s in &context_sources {
            seen_ids.insert(s.id);
        }
        for s in db_sources {
            if seen_ids.insert(s.id) {
                context_sources.push(s);
            }
        }

        // Step 3: Build context
        self.send_status(&status_sender, "Synthesizing answer...")
            .await;
        let context = if context_sources.is_empty() {
            "No relevant sources found.".to_string()
        } else {
            context_sources
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    format!(
                        "[Source {}]\nTitle: {}\nURL: {}\nContent: {}\n",
                        i + 1,
                        s.title,
                        s.url,
                        s.content.chars().take(2000).collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n---\n\n")
        };

        // Step 4: Query AI with the appropriate prompt style.
        let system_prompt = if agentic_search {
            format!(
                "You are an advanced AI assistant with research capabilities.\n\
                \n\
                TASK: Answer the user's query using ONLY the provided sources.\n\
                \n\
                GUIDELINES:\n\
                1. CITATIONS: Use [Source N] to cite information. Every fact must be cited.\n\
                2. SYNTHESIS: Combine information from multiple sources to provide a comprehensive answer.\n\
                3. HONESTY: If the sources do not contain the answer, state that clearly.\n\
                4. TEMPORAL AWARENESS: Current date is {}.\n\
                \n\
                SOURCES:\n{}",
                chrono::Utc::now().format("%Y-%m-%d"),
                context
            )
        } else if native_search {
            if context_sources.is_empty() {
                "You are a helpful AI assistant with native web search capability. Answer the user's question directly and clearly. If you use your own search, keep the response grounded and current.".to_string()
            } else {
                format!(
                    "You are a helpful AI assistant with native web search capability.\n\
                    \n\
                    Use the provided sources when relevant, and use your built-in search for anything missing.\n\
                    If you cite the provided sources, use [Source N].\n\
                    \n\
                    SOURCES:\n{}",
                    context
                )
            }
        } else {
            format!(
                "You are a helpful AI assistant with access to stored knowledge.\n\
                \n\
                TASK: Answer the user's query using the provided sources if relevant.\n\
                \n\
                GUIDELINES:\n\
                1. Prioritize the provided sources.\n\
                2. If sources are insufficient, you may use your training knowledge but must clarify what is from sources vs training.\n\
                3. Cite sources using [Source N].\n\
                \n\
                SOURCES:\n{}",
                context
            )
        };

        let mut messages: Vec<Value> = vec![json!({
            "role": "system",
            "content": system_prompt
        })];

        // Append history (limit to last 6 messages to save context)
        for msg in history.iter().rev().take(6).rev() {
            messages.push(json!({
                "role": msg.role,
                "content": msg.content
            }));
        }

        messages.push(json!({
            "role": "user",
            "content": user_query
        }));

        let tools = if self.model.supports_tools {
            Some(Tools::get_tools_definition())
        } else {
            None
        };
        tracing::info!(
            "Starting AI query with {} tools available",
            tools.as_ref().map(|t| t.len()).unwrap_or(0)
        );

        // Handle tool calling loop (max 3 iterations)
        let mut max_iterations = 3;
        let mut final_answer = String::new();

        while max_iterations > 0 {
            tracing::info!(
                "AI query iteration {} (remaining: {})",
                4 - max_iterations,
                max_iterations - 1
            );

            let response_json = self
                .llm_manager
                .chat_completion(
                    &self.model.id,
                    messages.clone(),
                    tools.clone(),
                    search_reasoning_enabled && self.model.supports_reasoning,
                )
                .await?;

            tracing::debug!(
                "Provider response: {}",
                serde_json::to_string_pretty(&response_json).unwrap_or_default()
            );

            if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
                if choices.is_empty() {
                    tracing::warn!("Provider returned empty choices array");
                    max_iterations -= 1;
                    continue;
                }

                if let Some(choice) = choices.first() {
                    if let Some(message) = choice.get("message") {
                        // Check if there's content (final answer)
                        if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                            if !content.is_empty() {
                                tracing::info!(
                                    "Received final answer from AI (length: {} chars)",
                                    content.len()
                                );
                                final_answer = content.to_string();
                                break;
                            }
                        }

                        // Check for tool calls
                        if let Some(tool_calls) =
                            message.get("tool_calls").and_then(|tc| tc.as_array())
                        {
                            if !tool_calls.is_empty() {
                                self.send_status(&status_sender, "Using calculation tools...")
                                    .await;
                                tracing::info!("AI requested {} tool calls", tool_calls.len());

                                // Keep the assistant tool-call message before any tool responses.
                                // Some providers are strict about assistant/tool alternation.
                                messages.push(message.clone());

                                // Execute tools and add responses
                                for (idx, tool_call) in tool_calls.iter().enumerate() {
                                    if let Some(function) = tool_call.get("function") {
                                        let function_name = function
                                            .get("name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("");

                                        let arguments_str = function
                                            .get("arguments")
                                            .and_then(|a| a.as_str())
                                            .unwrap_or("{}");

                                        tracing::info!(
                                            "Tool call {}: {} with args: {}",
                                            idx + 1,
                                            function_name,
                                            arguments_str
                                        );

                                        let arguments: Value = match serde_json::from_str(
                                            arguments_str,
                                        ) {
                                            Ok(args) => args,
                                            Err(e) => {
                                                tracing::warn!("Failed to parse tool arguments: {}, using empty object", e);
                                                json!({})
                                            }
                                        };

                                        let tool_result = match Tools::execute_tool(
                                            function_name,
                                            &arguments,
                                        ) {
                                            Ok(result) => {
                                                tracing::info!("Tool {} executed successfully, result length: {}", function_name, result.len());
                                                result
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    "Tool {} execution error: {}",
                                                    function_name,
                                                    e
                                                );
                                                format!("Error executing {}: {}", function_name, e)
                                            }
                                        };

                                        let tool_call_id = tool_call
                                            .get("id")
                                            .and_then(|id| id.as_str())
                                            .unwrap_or("");

                                        // Add tool response message
                                        messages.push(json!({
                                        "role": "tool",
                                        "content": tool_result,
                                        "tool_call_id": tool_call_id
                                        }));
                                    } else {
                                        tracing::warn!(
                                            "Tool call {} missing function field",
                                            idx + 1
                                        );
                                    }
                                }

                                tracing::info!(
                                    "Preparing next iteration with {} messages",
                                    messages.len()
                                );
                                max_iterations -= 1;
                                continue;
                            }
                        }

                        // Check finish_reason
                        if let Some(finish_reason) =
                            choice.get("finish_reason").and_then(|fr| fr.as_str())
                        {
                            tracing::info!("AI finished with reason: {}", finish_reason);
                            if finish_reason == "stop" {
                                // Try to get content even if not in message.content
                                if let Some(content) =
                                    message.get("content").and_then(|c| c.as_str())
                                {
                                    if !content.is_empty() {
                                        final_answer = content.to_string();
                                        break;
                                    }
                                }
                            }
                        }
                    } else {
                        tracing::warn!("Choice missing message field");
                    }
                }
            } else {
                tracing::warn!("Provider response missing choices field");
                if let Some(error) = response_json.get("error") {
                    tracing::error!(
                        "Provider API error: {}",
                        serde_json::to_string(error).unwrap_or_default()
                    );
                    return Err(anyhow::anyhow!(
                        "Provider API error: {}",
                        serde_json::to_string(error).unwrap_or_default()
                    ));
                }
            }

            max_iterations -= 1;
            tracing::warn!(
                "No valid response extracted, remaining iterations: {}",
                max_iterations
            );
        }

        if final_answer.is_empty() {
            tracing::warn!("No answer generated after {} iterations", 3);
            final_answer = "Sorry, I couldn't generate a response. Please try again.".to_string();
        } else {
            tracing::info!(
                "Successfully generated answer (length: {} chars)",
                final_answer.len()
            );
        }

        Ok((final_answer, context_sources))
    }
}
