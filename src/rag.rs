use crate::db::Database;
use crate::llm::LLMManager;
use crate::search::WebSearch;
use crate::tools::Tools;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QueryType {
    Factual,
    Comparative,
    HowTo,
    Explanation,
    News,
    Opinion,
    General,
}

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

    /// Classify query type for intelligent routing (I-RAG)
    fn classify_query(query: &str) -> QueryType {
        let q = query.to_lowercase();
        if q.contains("compare") || q.contains("difference") || q.contains("versus") || q.contains("vs ") {
            QueryType::Comparative
        } else if q.contains("how to") || q.contains("how do") || q.contains("guide") || q.contains("tutorial") {
            QueryType::HowTo
        } else if q.contains("why") || q.contains("reason") || q.contains("explain") {
            QueryType::Explanation
        } else if q.contains("news") || q.contains("recent") || q.contains("latest") || q.contains("breaking") {
            QueryType::News
        } else if q.contains("opinion") || q.contains("think") || q.contains("best") || q.contains("recommend") {
            QueryType::Opinion
        } else if q.contains("who is") || q.contains("what is") || q.contains("when did") || q.contains("where is") || q.contains("current") || q.contains("president") || q.contains("leader") {
            QueryType::Factual
        } else {
            QueryType::General
        }
    }

    /// Calculate confidence score based on source coverage (Agentic RAG)
    fn calculate_confidence(answer: &str, sources: &[crate::models::Source]) -> f64 {
        if sources.is_empty() {
            return 0.2;
        }
        let answer_lower = answer.to_lowercase();
        let mut covered_facts = 0;
        let _total_claims = answer_lower.matches('.').count().max(1);
        
        for source in sources {
            let content = source.content.to_lowercase();
            let key_terms: Vec<&str> = content.split_whitespace()
                .filter(|w| w.len() > 5)
                .take(20)
                .collect();
            for term in key_terms {
                if answer_lower.contains(term) {
                    covered_facts += 1;
                    break;
                }
            }
        }
        
        let source_coverage = (covered_facts as f64 / sources.len() as f64).min(1.0);
        0.3 + (source_coverage * 0.7)
    }

    /// Ask the LLM to plan the research steps with enhanced intelligence
    async fn plan_search(&self, query: &str, reasoning_enabled: bool) -> Result<Vec<String>> {
        tracing::info!("Planning search for query: {}", query);

        let current_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let _current_year = chrono::Utc::now().format("%Y").to_string();

        // Detect query type for smarter planning
        let query_lower = query.to_lowercase();
        
        // Analyze query intent
        let is_comparative = query_lower.contains("compare") 
            || query_lower.contains("difference") 
            || query_lower.contains("versus")
            || query_lower.contains("vs ");
        let is_factual = query_lower.contains("what is") 
            || query_lower.contains("who is") 
            || query_lower.contains("where is")
            || query_lower.contains("when did");
        let is_howto = query_lower.contains("how to") 
            || query_lower.contains("how do")
            || query_lower.contains("guide");
        let is_news = query_lower.contains("news") 
            || query_lower.contains("latest")
            || query_lower.contains("recently")
            || query_lower.contains("breaking");
        let is_buy = query_lower.contains("buy") 
            || query_lower.contains("price")
            || query_lower.contains("cost")
            || query_lower.contains("cheap");

        let system_prompt = if reasoning_enabled {
            // Enhanced reasoning for complex queries
            format!(
                "You are an expert research planner with deep understanding of information retrieval. Given a user query, generate 2-5 complementary search queries that would comprehensively answer it.\n\
                \n\
                Current date: {} (use this to understand 'current', 'recent', 'latest' queries)\n\
                \n\
                Query Analysis:\n\
                - Is comparative: {}\n\
                - Is factual: {}\n\
                - Is how-to: {}\n\
                - Is news-related: {}\n\
                - Is buying/research: {}\n\
                \n\
                Guidelines:\n\
                1. For comparative queries: Search BOTH sides and include comparison terms\n\
                2. For factual queries: Include exact terms and alternative phrasings\n\
                3. For news: Include date ranges and 'latest' terms\n\
                4. For how-to: Include step-by-step and tutorial terms\n\
                5. Always check for recent updates when information may change\n\
                6. Include one counterexample check when relevant\n\
                \n\
                Return ONLY a JSON object with a 'queries' key containing the list of strings.\n\
                Example: {{\"queries\": [\"current president of US 2024\", \"US president term length election\"]}}",
                current_date, is_comparative, is_factual, is_howto, is_news, is_buy
            )
        } else {
            // Simplified for faster queries
            format!(
                "You are a research planner. Given a user query, generate 1-3 specific search queries that would help answer it.\n\
                Current date: {}\n\
                Include date/context terms for time-sensitive queries.\n\
                Return ONLY a JSON object with a 'queries' key.\n\
                Example: {{\"queries\": [\"current president of US\"]}}",
                current_date
            )
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

        // Also check for reasoning_content (Pollinations AI)
        let reasoning = json_resp["choices"][0]["message"]["reasoning_content"]
            .as_str();

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

        // Fallback: try reasoning content if main content failed
        if let Some(reasoning_str) = reasoning {
            if let Ok(plan) = serde_json::from_str::<Value>(reasoning_str) {
                if let Some(queries) = plan["queries"].as_array() {
                    let strings: Vec<String> = queries
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    if !strings.is_empty() {
                        tracing::info!("Generated search plan from reasoning: {:?}", strings);
                        return Ok(strings);
                    }
                }
            }
        }

        // Fallback: just use the original query with date context
        Ok(vec![Self::enhance_query_with_temporal_context(query)])
    }

    /// Enhance search query with temporal context for time-sensitive queries
    fn enhance_query_with_temporal_context(query: &str) -> String {
        let _current_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let _current_year = chrono::Utc::now().format("%Y").to_string();
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

        // I-RAG: Classify query type for intelligent routing
        let query_type = Self::classify_query(user_query);
        tracing::info!("Query classified as: {:?}", query_type);

        // Adjust search strategy based on query type
        let search_depth = match query_type {
            QueryType::Comparative | QueryType::Explanation => 2, // More sources for comparison
            QueryType::News | QueryType::Factual => 1, // Standard
            _ => 1,
        };

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
        let current_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
        
        let system_prompt = if agentic_search {
            format!(
                "You are an advanced AI assistant with research capabilities.\n\
                \n\
                CURRENT DATE: {} (use this to answer 'current', 'latest', 'today' questions)\n\
                \n\
                TASK: Answer the user's query using the provided sources as your PRIMARY information source.\n\
                \n\
                PRIORITY RULES:\n\
                1. ALWAYS prioritize the provided sources over your training data\n\
                2. If sources contain the answer, cite them with [Source N]\n\
                3. If sources are INSUFFICIENT or OUTDATED, you MAY use your knowledge but clearly state: \"Based on my knowledge...\"\n\
                4. For 'current' or 'recent' questions about leaders/presidents: The current US President is Donald Trump (inaugurated January 20, 2025). Canada has Mark Carney as Prime Minister (started March 2025).\n\
                5. For factual questions, prefer the most recent source date\n\
                \n\
                OUTPUT FORMAT:\n\
                - Start with direct answer\n\
                - Cite sources using [Source N]\n\
                - If using your own knowledge, explicitly state it\n\
                \n\
                SOURCES:\n{}",
                current_date,
                context
            )
        } else if native_search {
            if context_sources.is_empty() {
                format!(
                    "You are a helpful AI assistant with native web search capability.\n\
                    CURRENT DATE: {}\n\
                    Answer the user's question directly and clearly.\n\
                    Keep your response grounded in current information.\n\
                    For 'current' questions about world leaders: US President is Donald Trump (since Jan 2025), Canada's PM is Mark Carney (since March 2025).",
                    current_date
                )
            } else {
                format!(
                    "You are a helpful AI assistant with native web search capability.\n\
                    CURRENT DATE: {}\n\
                    \n\
                    Use the provided sources as primary reference. Use your built-in search for anything missing.\n\
                    If you cite provided sources, use [Source N].\n\
                    For 'current' leaders: US President Donald Trump (Jan 2025), Canada PM Mark Carney (March 2025).\n\
                    \n\
                    SOURCES:\n{}",
                    current_date,
                    context
                )
            }
        } else {
            format!(
                "You are a helpful AI assistant with access to stored knowledge.\n\
                CURRENT DATE: {}\n\
                \n\
                TASK: Answer the user's query using the provided sources if relevant.\n\
                \n\
                GUIDELINES:\n\
                1. Prioritize the provided sources\n\
                2. If sources are insufficient, you may use your training knowledge but must clarify\n\
                3. Cite sources using [Source N]\n\
                \n\
                SOURCES:\n{}",
                current_date,
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

            // DEBUG: Log full response structure
            tracing::debug!(
                "Response JSON keys: {:?}",
                response_json.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
            
            // Try to extract message - check multiple paths
            let msg = response_json.get("choices")
                .and_then(|c| c.as_array())
                .and_then(|arr| arr.first())
                .and_then(|c| c.get("message"))
                .or_else(|| 
                    response_json.get("choices")
                    .and_then(|c| c.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|c| c.get("delta"))
                );
            
            if let Some(m) = msg {
                tracing::debug!("Message fields: {:?}", m.as_object().map(|o| o.keys().collect::<Vec<_>>()));
                
                // Log content if present
                if let Some(c) = m.get("content") {
                    tracing::debug!("Content field type: {:?}", c);
                    if let Some(s) = c.as_str() {
                        tracing::debug!("Content string: {}", s.chars().take(100).collect::<String>());
                    } else if let Some(obj) = c.as_object() {
                        tracing::debug!("Content is object with keys: {:?}", obj.keys().collect::<Vec<_>>());
                    }
                }
            }

            tracing::debug!(
                "Provider response: {}",
                serde_json::to_string_pretty(&response_json).unwrap_or_default()
            );

            // Check for error in response
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

            if let Some(choices) = response_json.get("choices").and_then(|c| c.as_array()) {
                if choices.is_empty() {
                    tracing::warn!("Provider returned empty choices array");
                    max_iterations -= 1;
                    continue;
                }

                if let Some(choice) = choices.first() {
                    // Try to get message from choice - could be nested differently
                    let message = choice.get("message")
                        .or_else(|| choice.get("delta"));  // delta is used in streaming
                    
                    if let Some(msg) = message {
                        // Extract content from various possible locations
                        let mut extracted = String::new();
                        
                        // 1. content field (string)
                        if let Some(c) = msg.get("content").and_then(|c| c.as_str()) {
                            if !c.is_empty() { extracted = c.to_string(); }
                        }
                        
                        // 2. content.text (object)
                        if extracted.is_empty() {
                            if let Some(c) = msg.get("content").and_then(|c| c.get("text")).and_then(|t| t.as_str()) {
                                if !c.is_empty() { extracted = c.to_string(); }
                            }
                        }
                        
                        // 3. text field directly
                        if extracted.is_empty() {
                            if let Some(t) = msg.get("text").and_then(|t| t.as_str()) {
                                if !t.is_empty() { extracted = t.to_string(); }
                            }
                        }
                        
                        // 4. reasoning field (Pollinations with reasoning)
                        if extracted.is_empty() {
                            if let Some(r) = msg.get("reasoning").and_then(|r| r.as_str()) {
                                if !r.is_empty() { 
                                    extracted = r.to_string();
                                    tracing::info!("Using reasoning field as answer (len: {})", r.len());
                                }
                            }
                        }
                        
                        // 5. reasoning_content field
                        if extracted.is_empty() {
                            if let Some(r) = msg.get("reasoning_content").and_then(|r| r.as_str()) {
                                if !r.is_empty() { 
                                    extracted = r.to_string();
                                    tracing::info!("Using reasoning_content field as answer (len: {})", r.len());
                                }
                            }
                        }
                        
                        // If we got content, use it
                        if !extracted.is_empty() {
                            final_answer = extracted;
                            break;
                        }
                        
                        // Check for tool calls
                        if let Some(tc) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                            if !tc.is_empty() {
                                self.send_status(&status_sender, "Using tools...").await;
                                messages.push(msg.clone());
                                
                                for tool_call in tc.iter() {
                                    if let Some(function) = tool_call.get("function") {
                                        let name = function.get("name").and_then(|n| n.as_str()).unwrap_or("");
                                        let args = function.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                                        
                                        let result = match Tools::execute_tool(name, &serde_json::from_str(args).unwrap_or(json!({}))) {
                                            Ok(r) => r,
                                            Err(e) => format!("Error: {}", e)
                                        };
                                        
                                        messages.push(json!({
                                            "role": "tool",
                                            "content": result
                                        }));
                                    }
                                }
                                max_iterations -= 1;
                                continue;
                            }
                        }
                        
                        // Check finish_reason for stop
                        if let Some(fr) = choice.get("finish_reason").and_then(|fr| fr.as_str()) {
                            if fr == "stop" {
                                // Try one more time to get content even if it was null
                                if extracted.is_empty() {
                                    // Last resort - check if there's any string field we missed
                                    tracing::warn!("finish_reason=stop but no content found");
                                }
                            }
                        }
                    } else {
                        tracing::warn!("Choice has no message/delta field");
                    }
                } else {
                    tracing::warn!("No first choice in response");
                }
            } else {
                tracing::warn!("No choices array in response");
            }

            max_iterations -= 1;
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

        // AGENTIC RAG: Verify answer against sources
        if agentic_search && !final_answer.is_empty() {
            let confidence = Self::calculate_confidence(&final_answer, &context_sources);
            tracing::info!("Answer confidence: {:.2}", confidence);
            
            if confidence < 0.5 {
                self.send_status(&status_sender, "Verifying answer accuracy...")
                    .await;
                
                if !context_sources.is_empty() {
                    let verification_context: String = context_sources.iter()
                        .take(3)
                        .map(|s| format!("[{}]: {}", s.title, s.content.chars().take(500).collect::<String>()))
                        .collect::<Vec<_>>()
                        .join("\n");
                    
                    let verify_system = format!("You are a fact-checker. Verify if the answer is supported by the sources. If not, correct it. Sources:\n{}", verification_context);
                    let verify_msg = vec![
                        json!({"role": "system", "content": verify_system}),
                        json!({"role": "user", "content": format!("Verify this answer: {}", final_answer)})
                    ];
                    
                    if let Ok(verify_resp) = self.llm_manager.chat_completion(
                        &self.model.id,
                        verify_msg,
                        None,
                        false
                    ).await {
                        if let Some(choices) = verify_resp.get("choices").and_then(|c| c.as_array()) {
                            if let Some(choice) = choices.first() {
                                if let Some(message) = choice.get("message") {
                                    if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
                                        if content.len() < final_answer.len() * 2 && !content.contains("not supported") {
                                            final_answer = content.to_string();
                                            tracing::info!("Answer refined through verification");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok((final_answer, context_sources))
    }
}
