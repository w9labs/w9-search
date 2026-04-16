use anyhow::Result;
use scraper::{Html, Selector};
use regex::Regex;
use serde::Deserialize;
use std::env;
use crate::db::Database;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[async_trait::async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, db: &Database, query: &str) -> Result<Vec<SearchResult>>;
    fn name(&self) -> &str;
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn build_focus_terms(query_hint: Option<&str>, title_hint: Option<&str>) -> Vec<String> {
    let mut terms = Vec::new();
    for source in [query_hint, title_hint] {
        if let Some(text) = source {
            for raw in text
                .split(|c: char| !c.is_alphanumeric())
                .map(|s| s.trim().to_lowercase())
            {
                if raw.len() < 4 {
                    continue;
                }
                if matches!(
                    raw.as_str(),
                    "the" | "and" | "with" | "from" | "that" | "this" | "what" | "when" | "where" | "which" | "into" | "about"
                ) {
                    continue;
                }
                if !terms.iter().any(|existing: &String| existing == &raw) {
                    terms.push(raw);
                }
            }
        }
    }
    terms
}

fn focus_text_for_extraction(text: &str, terms: &[String], max_chars: usize) -> String {
    let normalized = normalize_whitespace(text);
    if normalized.len() <= max_chars {
        return normalized;
    }

    let splitter = Regex::new(r"(?<=[.!?。；;\n])\s+").unwrap();
    let mut scored = Vec::new();

    for sentence in splitter.split(&normalized) {
        let sentence = sentence.trim();
        if sentence.len() < 16 {
            continue;
        }

        let lower = sentence.to_lowercase();
        let hits = terms.iter().filter(|term| lower.contains(term.as_str())).count() as f64;
        let length_bonus = (sentence.len().min(500) as f64) / 500.0;
        let punctuation_bonus = if sentence.ends_with(':') { 0.15 } else { 0.0 };
        scored.push((hits * 2.5 + length_bonus + punctuation_bonus, sentence.to_string()));
    }

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut selected = Vec::new();
    let mut total_len = 0usize;
    for (_, sentence) in scored {
        let projected = total_len + sentence.len() + 1;
        if projected > max_chars {
            continue;
        }
        total_len = projected;
        selected.push(sentence);
        if total_len > max_chars / 2 {
            break;
        }
    }

    let focused = normalize_whitespace(&selected.join(" "));
    if focused.len() < 200 {
        normalized.chars().take(max_chars).collect()
    } else {
        focused
    }
}

fn extract_meta_description(document: &Html) -> Option<String> {
    let selectors = [
        "meta[name='description']",
        "meta[property='og:description']",
        "meta[name='twitter:description']",
    ];

    for selector_str in selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                if let Some(content) = element.value().attr("content") {
                    let normalized = normalize_whitespace(content);
                    if !normalized.is_empty() {
                        return Some(normalized);
                    }
                }
            }
        }
    }

    None
}

fn looks_blocked(html: &str) -> bool {
    let lower = html.to_lowercase();
    [
        "just a moment",
        "checking your browser",
        "enable javascript",
        "access denied",
        "captcha",
        "cloudflare",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

pub struct DuckDuckGoSearch;

#[async_trait::async_trait]
impl SearchProvider for DuckDuckGoSearch {
    fn name(&self) -> &str {
        "DuckDuckGo"
    }

    async fn search(&self, _db: &Database, query: &str) -> Result<Vec<SearchResult>> {
        let url = format!("https://html.duckduckgo.com/html/?q={}", 
            urlencoding::encode(query));
        
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .build()?;
        
        let html = client.get(&url).send().await?.text().await?;
        let document = Html::parse_document(&html);
        
        let result_selector = Selector::parse(".result").unwrap();
        let title_selector = Selector::parse(".result__a").unwrap();
        let snippet_selector = Selector::parse(".result__snippet").unwrap();
        
        let mut results = Vec::new();
        
        for result in document.select(&result_selector).take(5) {
            if let Some(title_elem) = result.select(&title_selector).next() {
                let title = title_elem.text().collect::<String>();
                let mut url = title_elem.value().attr("href")
                    .unwrap_or("")
                    .to_string();
                
                if url.starts_with("/l/?uddg=") {
                    if let Some(decoded) = url.strip_prefix("/l/?uddg=") {
                        if let Ok(decoded_url) = urlencoding::decode(decoded) {
                            url = decoded_url.to_string();
                        }
                    }
                }
                
                if url.starts_with("//") {
                    url = format!("https:{}", url);
                }
                
                if url.is_empty() || url.starts_with('/') || (!url.starts_with("http://") && !url.starts_with("https://")) {
                    continue;
                }
                
                let snippet = result.select(&snippet_selector)
                    .next()
                    .map(|e| e.text().collect::<String>())
                    .unwrap_or_default();
                
                if !title.is_empty() {
                    results.push(SearchResult {
                        title,
                        url,
                        snippet,
                    });
                }
            }
        }
        
        Ok(results)
    }
}

pub struct BraveSearch {
    api_key: String,
}

#[derive(Deserialize)]
struct BraveResponse {
    web: BraveWeb,
}

#[derive(Deserialize)]
struct BraveWeb {
    results: Vec<BraveResult>,
}

#[derive(Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    description: Option<String>,
}

#[async_trait::async_trait]
impl SearchProvider for BraveSearch {
    fn name(&self) -> &str {
        "Brave Search"
    }

    async fn search(&self, db: &Database, query: &str) -> Result<Vec<SearchResult>> {
        // Check rate limit (cost 1)
        if !db.check_search_rate_limit("search:brave", 1).await? {
            return Err(anyhow::anyhow!("Brave Search rate limit exceeded"));
        }

        let client = reqwest::Client::new();
        let response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .query(&[("q", query), ("count", "5")])
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .send()
            .await?;

        // Parse headers for rate limits
        let remaining_header = response.headers().get("x-ratelimit-remaining")
            .and_then(|h| h.to_str().ok());
        let limit_header = response.headers().get("x-ratelimit-limit")
            .and_then(|h| h.to_str().ok());
            
        if let (Some(rem_str), Some(lim_str)) = (remaining_header, limit_header) {
            // Format: "burst, month" e.g., "1, 2000"
            // We want the month part (second value)
            let rem_parts: Vec<&str> = rem_str.split(',').map(|s| s.trim()).collect();
            let lim_parts: Vec<&str> = lim_str.split(',').map(|s| s.trim()).collect();
            
            if rem_parts.len() >= 2 && lim_parts.len() >= 2 {
                if let (Ok(rem_month), Ok(lim_month)) = (rem_parts[1].parse::<i64>(), lim_parts[1].parse::<i64>()) {
                    let used_month = lim_month.saturating_sub(rem_month);
                    let _ = db.update_search_limits("search:brave", Some(used_month), Some(lim_month), None).await;
                }
            }
        }

        if !response.status().is_success() {
             return Err(anyhow::anyhow!("Brave Search API error: {}", response.status()));
        }

        let brave_resp: BraveResponse = response.json().await?;
        
        let results = brave_resp.web.results.into_iter().map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.description.unwrap_or_default(),
        }).collect();

        Ok(results)
    }
}

pub struct TavilySearch {
    api_key: String,
}

#[derive(Deserialize)]
struct TavilyResponse {
    results: Vec<TavilyResult>,
}

#[derive(Deserialize)]
struct TavilyResult {
    title: String,
    url: String,
    content: String,
}

#[async_trait::async_trait]
impl SearchProvider for TavilySearch {
    fn name(&self) -> &str {
        "Tavily"
    }

    async fn search(&self, db: &Database, query: &str) -> Result<Vec<SearchResult>> {
        // Check rate limit (cost 1 for basic search)
        if !db.check_search_rate_limit("search:tavily", 1).await? {
            return Err(anyhow::anyhow!("Tavily rate limit exceeded"));
        }

        let client = reqwest::Client::new();
        let response = client
            .post("https://api.tavily.com/search")
            .json(&serde_json::json!({
                "api_key": self.api_key,
                "query": query,
                "search_depth": "basic",
                "max_results": 5
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Tavily API error: {}", response.status()));
        }

        let tavily_resp: TavilyResponse = response.json().await?;

        let results = tavily_resp.results.into_iter().map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.content, // Tavily returns 'content' which is a snippet
        }).collect();

        Ok(results)
    }
}

pub struct SearXNGSearch {
    base_url: String,
}

#[derive(Deserialize)]
struct SearXNGResponse {
    results: Vec<SearXNGResult>,
}

#[derive(Deserialize)]
struct SearXNGResult {
    title: String,
    url: String,
    content: Option<String>,
}

#[async_trait::async_trait]
impl SearchProvider for SearXNGSearch {
    fn name(&self) -> &str {
        "SearXNG"
    }

    async fn search(&self, _db: &Database, query: &str) -> Result<Vec<SearchResult>> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()?;
            
        let base = self.base_url.trim_end_matches('/');
        let url = if base.ends_with("/search") {
            base.to_string()
        } else {
            format!("{}/search", base)
        };
        
        tracing::debug!("SearXNG URL: {}", url);
        
        let response = client
            .get(&url)
            .query(&[
                ("q", query),
                ("format", "json"),
                ("language", "auto"),
                ("safesearch", "1"),
            ])
            // Add headers to satisfy SearXNG bot detection
            .header("X-Forwarded-For", "127.0.0.1") 
            .header("User-Agent", "w9-search/1.0")
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::warn!("SearXNG API error: {} - Body: {}", status, text);
            return Err(anyhow::anyhow!("SearXNG API error: {}", status));
        }

        let text = response.text().await?;
        tracing::debug!("SearXNG response: {}", text.chars().take(200).collect::<String>());
        
        let searx_resp: SearXNGResponse = serde_json::from_str(&text).map_err(|e| {
            tracing::error!("Failed to parse SearXNG response: {}", e);
            e
        })?;

        let results = searx_resp.results.into_iter().map(|r| SearchResult {
            title: r.title,
            url: r.url,
            snippet: r.content.unwrap_or_default(),
        }).collect();

        Ok(results)
    }
}

pub struct WebSearch;

impl WebSearch {
    fn searxng_base_url() -> String {
        env::var("SEARXNG_BASE_URL")
            .ok()
            .filter(|url| !url.trim().is_empty())
            .unwrap_or_else(|| "https://searxng.w9.nu".to_string())
    }

    pub async fn get_provider(name: Option<&str>) -> Box<dyn SearchProvider> {
        // If a specific provider is requested, try to use it if configured
        if let Some(n) = name {
            match n.to_lowercase().as_str() {
                "searxng" => {
                    return Box::new(SearXNGSearch { base_url: Self::searxng_base_url() });
                },
                "tavily" => {
                    if let Ok(key) = env::var("TAVILY_API_KEY") {
                        if !key.is_empty() {
                            return Box::new(TavilySearch { api_key: key });
                        }
                    }
                },
                "brave" => {
                    if let Ok(key) = env::var("BRAVE_API_KEY") {
                        if !key.is_empty() {
                            return Box::new(BraveSearch { api_key: key });
                        }
                    }
                },
                "duckduckgo" | "ddg" => return Box::new(DuckDuckGoSearch),
                _ => {} // Fall through to auto
            }
        }

        // Auto logic (Priority: SearXNG -> Tavily -> Brave -> DDG)
        let searxng_url = Self::searxng_base_url();
        if !searxng_url.is_empty() {
            return Box::new(SearXNGSearch { base_url: searxng_url });
        }

        if let Ok(key) = env::var("TAVILY_API_KEY") {
            if !key.is_empty() {
                return Box::new(TavilySearch { api_key: key });
            }
        }
        
        if let Ok(key) = env::var("BRAVE_API_KEY") {
             if !key.is_empty() {
                return Box::new(BraveSearch { api_key: key });
            }
        }
        
        Box::new(DuckDuckGoSearch)
    }


    pub async fn search(db: &Database, query: &str, provider: Option<&str>) -> Result<Vec<SearchResult>> {
        let provider = Self::get_provider(provider).await;
        tracing::info!("Using search provider: {}", provider.name());
        provider.search(db, query).await
    }
    
    pub async fn sync_tavily_usage(db: &Database) -> Result<()> {
        if let Ok(key) = env::var("TAVILY_API_KEY") {
            if key.is_empty() { return Ok(()); }
            
            tracing::info!("Syncing Tavily usage...");
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?;
            
            let response = client.get("https://api.tavily.com/usage")
                .header("Authorization", format!("Bearer {}", key))
                .send()
                .await?;
                
            if response.status().is_success() {
                let json: serde_json::Value = response.json().await?;
                // Response body: { "key": { "usage": 150, "limit": 1000, ... } }
                if let Some(k) = json.get("key") {
                    let usage = k.get("usage").and_then(|v| v.as_i64());
                    let limit = k.get("limit").and_then(|v| v.as_i64());
                    
                    if let (Some(u), Some(l)) = (usage, limit) {
                        tracing::info!("Tavily usage: {}/{}", u, l);
                        db.update_search_limits("search:tavily", Some(u), Some(l), None).await?;
                    }
                }
            } else {
                tracing::warn!("Failed to sync Tavily usage: {}", response.status());
            }
        }
        Ok(())
    }
    
    pub async fn fetch_content(
        url: &str,
        query_hint: Option<&str>,
        snippet_hint: Option<&str>,
        title_hint: Option<&str>,
    ) -> Result<String> {
        let normalized_url = if url.starts_with("//") {
            format!("https:{}", url)
        } else if url.starts_with('/') {
            return Err(anyhow::anyhow!("Relative URL not supported: {}", url));
        } else if !url.starts_with("http://") && !url.starts_with("https://") {
            format!("https://{}", url)
        } else {
            url.to_string()
        };
        
        tracing::debug!("Fetching content from: {}", normalized_url);
        
        let client = reqwest::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .timeout(std::time::Duration::from_secs(20))
            .build()?;
        
        let html = client
            .get(&normalized_url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Accept-Language", "en-US,en;q=0.8")
            .header("Cache-Control", "no-cache")
            .send()
            .await?
            .text()
            .await?;

        if looks_blocked(&html) {
            return Err(anyhow::anyhow!("Blocked or challenge page at {}", normalized_url));
        }

        let document = Html::parse_document(&html);

        let terms = build_focus_terms(query_hint, title_hint);
        let candidate_selectors = [
            "article",
            "main",
            "[role='main']",
            "[itemprop='articleBody']",
            "#content",
            ".content",
            ".article-content",
            ".entry-content",
            ".post-content",
            ".post-body",
            "body",
        ];

        let block_selector = Selector::parse("p, h1, h2, h3, h4, h5, h6, li, blockquote, section, div").unwrap();
        let link_selector = Selector::parse("a").unwrap();
        let mut best_text = String::new();
        let mut best_score = f64::MIN;

        for selector_str in candidate_selectors {
            if let Ok(selector) = Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    let mut blocks = Vec::new();
                    for block in element.select(&block_selector) {
                        let text = normalize_whitespace(&block.text().collect::<Vec<_>>().join(" "));
                        if text.len() < 40 {
                            continue;
                        }

                        let link_text_len: usize = block
                            .select(&link_selector)
                            .map(|link| link.text().collect::<Vec<_>>().join(" ").len())
                            .sum();
                        let link_density = if text.is_empty() {
                            1.0
                        } else {
                            link_text_len as f64 / text.len() as f64
                        };
                        if link_density > 0.55 {
                            continue;
                        }

                        if let Some(class_attr) = block.value().attr("class") {
                            let lower = class_attr.to_lowercase();
                            if lower.contains("nav")
                                || lower.contains("menu")
                                || lower.contains("footer")
                                || lower.contains("header")
                                || lower.contains("sidebar")
                            {
                                continue;
                            }
                        }

                        blocks.push(text);
                    }

                    let joined = normalize_whitespace(&blocks.join("\n"));
                    if joined.len() < 100 {
                        continue;
                    }

                    let lower = joined.to_lowercase();
                    let mut score = (joined.len().min(4000) as f64) / 100.0;
                    score += terms
                        .iter()
                        .filter(|term| lower.contains(term.as_str()))
                        .count() as f64
                        * 6.0;
                    if selector_str == "article" || selector_str == "main" {
                        score += 8.0;
                    }
                    if let Some(title) = title_hint {
                        if lower.contains(&title.to_lowercase()) {
                            score += 4.0;
                        }
                    }

                    if score > best_score {
                        best_score = score;
                        best_text = joined;
                    }
                }
            }
        }

        if best_text.is_empty() {
            if let Some(meta) = extract_meta_description(&document) {
                best_text = meta;
            } else {
                best_text = document.root_element().text().collect::<Vec<_>>().join(" ");
            }
        }

        let mut content = focus_text_for_extraction(&best_text, &terms, 12_000);
        if content.len() < 180 {
            if let Some(snippet) = snippet_hint {
                let snippet = normalize_whitespace(snippet);
                if !snippet.is_empty() {
                    content = match title_hint {
                        Some(title) if !title.trim().is_empty() => {
                            format!("{}\n\n{}", title.trim(), snippet)
                        }
                        _ => snippet,
                    };
                }
            }
        }

        if content.len() > 15_000 {
            let mut limit = 15_000;
            while !content.is_char_boundary(limit) {
                limit -= 1;
            }
            content.truncate(limit);
        }

        Ok(content)
    }
}
