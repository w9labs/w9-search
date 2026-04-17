use axum::{
    extract::{Form, Path, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect, Response},
};
use maud::{html, Markup, DOCTYPE};
use serde::Deserialize;
use std::collections::HashMap;
use tracing;

use crate::{
    auth,
    llm::{Model, ProviderType},
    AppState,
};

fn model_option_label(model: &Model) -> String {
    let mut tags = Vec::new();
    if model.supports_native_search {
        tags.push("search");
    }
    if model.supports_reasoning {
        tags.push("reasoning");
    }
    if model.supports_tools {
        tags.push("tools");
    }
    if model.is_free {
        tags.push("free");
    }

    if tags.is_empty() {
        model.name.clone()
    } else {
        format!("{} [{}]", model.name, tags.join(", "))
    }
}

fn auth_page(title: &str, subtitle: &str, nav: Markup, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                link rel="icon" href="/static/favicon.svg" type="image/svg+xml";
                link rel="stylesheet" href="/static/style.css";
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link href=(r#"https://fonts.googleapis.com/css2?family=Press+Start+2P&family=VT323&display=swap"#) rel="stylesheet";
            }
            body class="auth-page" {
                div class="auth-shell" {
                    header {
                        img class="brand-banner" src="/static/w9-search-banner.svg" alt="W9 Search";
                        h1 { (title) }
                        p class="subtitle" { (subtitle) }
                        nav { (nav) }
                    }
                    (content)
                }
            }
        }
    }
}

fn public_landing_page() -> Markup {
    auth_page(
        "W9 Search",
        "Public landing, role-aware chat, and auto routing without surprise redirects.",
        html! {
            a href="/login" class="nav-link" { "Login" }
            a href="#features" class="nav-link" { "Features" }
        },
        html! {
            div class="section" {
                div class="card" {
                    div class="card-header" {
                        span class="provider-badge" { "Public" }
                        h3 { "Search without auto login" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "search.w9.nu opens here first. Nothing signs you in until you ask for it."
                        }
                        div class="auth-actions" {
                            a href="/login" class="cta-link" { "Sign in with W9 DB" }
                            a href="#features" class="cta-link secondary" { "See features" }
                        }
                    }
                }
            }

            div id="features" class="auth-grid" {
                div class="card" {
                    div class="card-header" {
                        span class="badge badge--ok" { "Auto" }
                        h3 { "Smart routing" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "Auto mode chooses the best allowed model and search backend for each query."
                        }
                    }
                }
                div class="card" {
                    div class="card-header" {
                        span class="badge badge--warn" { "Search" }
                        h3 { "Native search aware" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "Models with native search skip local web search; unsupported models get an agentic search path."
                        }
                    }
                }
                div class="card" {
                    div class="card-header" {
                        span class="badge badge--warn" { "Roles" }
                        h3 { "Admin / Dev model picker" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "Admin and dev accounts can pick a model inside chat. Everyone else stays on Auto (Smart)."
                        }
                    }
                }
                div class="card" {
                    div class="card-header" {
                        span class="badge badge--ok" { "Mobile" }
                        h3 { "Responsive by default" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "The landing page, login page, and chat shell are tuned for desktop and mobile."
                        }
                    }
                }
            }

            div class="section" {
                div class="card" {
                    div class="card-header" {
                        span class="provider-badge" { "Access" }
                        h3 { "How login works" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "Use the W9 DB login page when you want to start chatting. The callback closes the popup and returns you here."
                        }
                    }
                }
            }
        },
    )
}

fn login_page_markup(session: Option<&auth::UserSession>) -> Markup {
    let nav = html! {
        a href="/" class="nav-link" { "Home" }
        a href="#access" class="nav-link" { "Access" }
    };

    let content = if let Some(session) = session {
        html! {
            div class="auth-grid" id="access" {
                div class="card" {
                    div class="card-header" {
                        span class="badge badge--ok" { "Signed in" }
                        h3 { "You already have access" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            (format!("Signed in as {} with the {} role.", session.email, session.role))
                        }
                        div class="auth-actions" {
                            a href="/" class="cta-link" { "Open Search" }
                            a href="/logout" class="cta-link secondary" { "Sign out" }
                        }
                    }
                }
                div class="card" {
                    div class="card-header" {
                        span class="provider-badge" { "Role" }
                        h3 { "What this account can do" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "Admin and dev accounts can choose a model inside chat. Everyone else stays on Auto (Smart)."
                        }
                        div class="model-badges" {
                            span class="badge badge--ok" { "Auto routing" }
                            span class="badge badge--warn" { "Model picker in chat" }
                        }
                    }
                }
            }
        }
    } else {
        html! {
            div class="auth-grid" {
                div class="card" {
                    div class="card-header" {
                        span class="badge badge--ok" { "Login" }
                        h3 { "Continue with W9 DB" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "Sign in to unlock chat, saved threads, and the auto-routed search pipeline."
                        }
                        div class="auth-actions" {
                            a href=(auth::login_url()) class="cta-link" onclick="const w = window.open(this.href, 'w9-search-login', 'width=520,height=720'); if (w) { w.focus(); return false; }" { "Open W9 DB Login" }
                            a href="/" class="cta-link secondary" { "Back to landing" }
                        }
                    }
                }
                div class="card" {
                    div class="card-header" {
                        span class="badge badge--warn" { "Advanced" }
                        h3 { "Role-based model access" }
                    }
                    div class="card-body" {
                        p class="auth-copy" {
                            "Admin and dev accounts can choose a model in chat. Everyone else uses Auto (Smart) for safety."
                        }
                        div class="model-badges" {
                            span class="badge badge--ok" { "Auto for everyone" }
                            span class="badge badge--warn" { "Manual models for admin/dev" }
                        }
                    }
                }
            }
        }
    };

    auth_page(
        "W9 Search Login",
        "A separate login page with popup sign-in and a clear return path.",
        nav,
        content,
    )
}

fn render_model_picker(session: &auth::UserSession, models: &[Model]) -> Markup {
    if !auth::can_choose_model_role(&session.role) {
        return html! {
            div class="control-group" {
                div class="provider-badge" { "Auto (Smart)" }
                input type="hidden" id="model-select" value="auto" {}
            }
        };
    }

    let mut grouped_models = Vec::new();
    for provider in ProviderType::all() {
        let mut provider_models: Vec<&Model> = models
            .iter()
            .filter(|model| model.provider == *provider)
            .collect();
        provider_models.sort_by(|a, b| a.name.cmp(&b.name));
        if !provider_models.is_empty() {
            grouped_models.push((*provider, provider_models));
        }
    }

    html! {
        div class="control-group model-picker" {
            span class="label" { "Model" }
            select id="model-select" class="model-select" {
                option value="auto" selected { "Auto (Smart)" }
                @for (provider, provider_models) in grouped_models {
                    optgroup label=(provider.to_string()) {
                        @for model in provider_models {
                            option value=(model.id.as_str()) {
                                (model_option_label(model))
                            }
                        }
                    }
                }
            }
        }
        @if models.is_empty() {
            span class="badge badge--warn" { "Model cache warming" }
        } @else {
            span class="badge badge--ok" { "Admin / Dev" }
        }
    }
}

pub async fn login(headers: HeaderMap) -> Response {
    let session = auth::require_session(&headers);
    Html(login_page_markup(session.as_ref()).into_string()).into_response()
}

pub async fn models(headers: HeaderMap, State(state): State<AppState>) -> Response {
    let Some(session) = auth::require_session(&headers) else {
        return Redirect::to("/login").into_response();
    };

    let metrics = state
        .db
        .get_all_provider_metrics()
        .await
        .unwrap_or_default();

    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "W9 Search - Auto Routing" }
                link rel="icon" href="/static/favicon.svg" type="image/svg+xml";
                link rel="stylesheet" href="/static/style.css";
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link href=(r#"https://fonts.googleapis.com/css2?family=Press+Start+2P&family=VT323&display=swap"#) rel="stylesheet";
            }
            body {
                div class="container" {
                    header {
                        img class="brand-banner" src="/static/w9-search-banner.svg" alt="W9 Search";
                        h1 { "W9 Search" }
                        p class="subtitle" { "Auto Routing & Limits" }
                        nav {
                            a href="/" class="nav-link" { "← Back to Search" }
                            @if session.role == "admin" {
                                a href="/admin" class="nav-link" { "Admin Panel" }
                            }
                            a href="/logout" class="nav-link" { "Logout" }
                        }
                    }

                    div class="section" {
                        h2 { "Provider Limits & Usage" }
                        div class="grid-container" {
                            @for metric in &metrics {
                                div class="metric-card" {
                                    div class="metric-title" { (metric.provider) }

                                    // Daily Requests
                                    div class="metric-row" {
                                        span class="metric-name" { "Daily Requests Left" }
                                        @let (used, limit, pct) = match (metric.req_day, metric.limit_day) {
                                            (Some(u), Some(l)) if l > 0 => (u, l, ((l as f64 - u as f64) / l as f64 * 100.0).max(0.0)),
                                            (Some(u), Some(_)) => (u, 0, 0.0),
                                            (Some(u), None) => (u, 0, 100.0),
                                            (None, Some(l)) => (0, l, 100.0),
                                            (None, None) => (0, 0, 100.0),
                                        };
                                        div class="progress-container" {
                                            div class="progress-bar" style=(format!("width: {}%", pct)) {}
                                        }
                                        div class="progress-label" {
                                            span {
                                                @if limit > 0 {
                                                    (format!("{}", limit.saturating_sub(used)))
                                                } @else {
                                                    "∞"
                                                }
                                            }
                                            span {
                                                @if limit > 0 {
                                                    (format!("Limit: {}", limit))
                                                } @else {
                                                    "Unlimited"
                                                }
                                            }
                                        }
                                    }

                                    // Minute Requests (RPM)
                                    div class="metric-row" {
                                        span class="metric-name" { "Requests Per Minute" }
                                        @let (used, limit, pct) = match (metric.req_min, metric.limit_min) {
                                            (Some(u), Some(l)) if l > 0 => (u, l, ((l as f64 - u as f64) / l as f64 * 100.0).max(0.0)),
                                            (Some(u), Some(_)) => (u, 0, 0.0),
                                            (Some(u), None) => (u, 0, 100.0),
                                            (None, Some(l)) => (0, l, 100.0),
                                            (None, None) => (0, 0, 100.0),
                                        };
                                        div class="progress-container" {
                                            div class="progress-bar" style=(format!("width: {}%", pct)) {}
                                        }
                                        div class="progress-label" {
                                            span {
                                                @if limit > 0 {
                                                    (format!("{} left", limit.saturating_sub(used)))
                                                } @else {
                                                    "∞"
                                                }
                                            }
                                            span {
                                                @if limit > 0 {
                                                    (format!("Limit: {}", limit))
                                                } @else {
                                                    "Unlimited"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div class="section" {
                        h2 { "Auto (Smart)" }
                        div class="card" {
                            div class="card-header" {
                                span class="provider-badge" { "Auto" }
                                h3 { "Smart selection only" }
                            }
                            div class="card-body" {
                                div class="meta-item" {
                                    span class="label" { "Signed in as:" }
                                    span { (session.email) }
                                }
                                div class="meta-item" {
                                    span class="label" { "Mode:" }
                                    span {
                                        @if auth::can_choose_model_role(&session.role) {
                                            "Model picker available in chat"
                                        } @else {
                                            "Auto (Smart) only for this account"
                                        }
                                    }
                                }
                                p class="text-muted" {
                                    @if auth::can_choose_model_role(&session.role) {
                                        "W9 Search keeps auto routing for everyone, and this account can switch models in chat when needed."
                                    } @else {
                                        "W9 Search now chooses the best provider automatically from the approved Pollinations set and the configured search backends."
                                    }
                                }
                                div class="model-badges" {
                                    span class="badge badge--ok" { "Auto route" }
                                    span class="badge badge--warn" { "Native search aware" }
                                    span class="badge badge--warn" { "Reasoning aware" }
                                    @if auth::can_choose_model_role(&session.role) {
                                        span class="badge badge--ok" { "Model picker in chat" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    Html(markup.into_string()).into_response()
}

pub async fn index(headers: HeaderMap, State(state): State<AppState>) -> Response {
    let Some(session) = auth::require_session(&headers) else {
        return Html(public_landing_page().into_string()).into_response();
    };

    let models = if auth::can_choose_model_role(&session.role) {
        state.llm_manager.get_models().await
    } else {
        Vec::new()
    };

    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "W9 Search" }
                link rel="icon" href="/static/favicon.svg" type="image/svg+xml";
                script src="https://cdn.jsdelivr.net/npm/marked@11.1.1/marked.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/mermaid@10.6.1/dist/mermaid.min.js" {}
                link rel="stylesheet" href="/static/style.css";
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link href=(r#"https://fonts.googleapis.com/css2?family=Press+Start+2P&family=VT323&display=swap"#) rel="stylesheet";
            }
            body {
                // Sidebar
                aside class="app-sidebar" {
                    div class="sidebar-header" {
                        div class="brand-shell" {
                            img class="brand-mark" src="/static/w9-search-mark.svg" alt="W9 Search";
                            div class="logo" { "W9 SEARCH" }
                            div class="provider-badge" { (session.email) }
                        }
                        button id="new-chat-btn" class="new-chat-btn" title="New Chat" { "+" }
                    }
                    div id="thread-list" class="thread-list" {
                        // Threads will be injected here
                    }
                        div class="sidebar-footer" {
                            div style="display:flex; flex-direction:column; gap:0.35rem;" {
                                a href="/models" class="nav-link" { "Auto Routing & Limits" }
                                @if session.role == "admin" {
                                    a href="/admin" class="nav-link" { "Admin Panel" }
                                }
                                a href="/logout" class="nav-link" { "Logout" }
                            }
                        }
                    }

                // Main Content
                main class="app-main" {
                    // Chat History
                    div id="chat-container" class="chat-container" {
                        div class="welcome-screen" {
                            h1 { "What do you want to know?" }
                            p { "Ask anything. I'll research it for you." }
                        }
                    }

                    // Input Area
                    div class="input-area" {
                        div class="settings-bar" {
                            div class="control-group" {
                                label class="toggle-switch" {
                                    input type="checkbox" id="web-search-toggle" checked {}
                                    span class="slider" {}
                                    span { "Web Search" }
                                }
                            }
                            div class="control-group" {
                                label class="toggle-switch" {
                                    input type="checkbox" id="search-reasoning-toggle" {}
                                    span class="slider" {}
                                    span { "Reasoning" }
                                }
                            }
                            (render_model_picker(&session, &models))
                            div class="control-group" {
                                select id="provider-select" {
                                    option value="auto" { "Auto Engine" }
                                    option value="searxng" { "SearXNG" }
                                    option value="tavily" { "Tavily" }
                                    option value="brave" { "Brave" }
                                    option value="ddg" { "DuckDuckGo" }
                                }
                            }
                        }
                        p class="search-note" {
                            @if auth::can_choose_model_role(&session.role) {
                                "Auto mode still picks the smartest allowed model by default. Admin and dev accounts can override it with the selector above."
                            } @else {
                                "Auto mode picks the smartest allowed model for each query. Native-search models skip local web search, and reasoning expands the planner when available."
                            }
                        }
                        div class="input-container" {
                            textarea id="user-input" placeholder="Type your follow-up question..." rows="1" {}
                            button id="send-btn" class="send-btn" { "→" }
                        }
                    }
                }

                script {
                    (maud::PreEscaped(r#"                    let currentThreadId = null;
                    let accumulatedSources = []; // Store sources for the current turn to look up for citations

                    // --- Initialization ---
                    document.addEventListener('DOMContentLoaded', () => {
                        mermaid.initialize({ startOnLoad: false, theme: 'dark' });
                        marked.setOptions({ breaks: true, gfm: true });
                        loadThreads();
                        document.getElementById('user-input').focus();
                    });

                    // --- Sidebar Logic ---
                    async function loadThreads() {
                        try {
                            const res = await fetch('/api/threads');
                            if (res.status === 401) {
                                window.location.href = '/login';
                                return;
                            }
                            const threads = await res.json();
                            const list = document.getElementById('thread-list');
                            list.innerHTML = '';
                            threads.forEach(t => {
                                const div = document.createElement('div');
                                div.className = 'thread-item';
                                
                                // Title
                                const titleSpan = document.createElement('span');
                                titleSpan.className = 'thread-title';
                                titleSpan.textContent = t.title || 'Untitled Chat';
                                titleSpan.onclick = () => loadThread(t.id);
                                div.appendChild(titleSpan);
                                
                                // Actions container
                                const actions = document.createElement('div');
                                actions.className = 'thread-actions';
                                
                                // Share button
                                const shareBtn = document.createElement('button');
                                shareBtn.className = 'thread-action-btn';
                                shareBtn.title = 'Share';
                                shareBtn.innerHTML = '▶';
                                shareBtn.onclick = async (e) => {
                                    e.stopPropagation();
                                    try {
                                        const res = await fetch(`/api/threads/${t.id}/share`, { method: 'POST' });
                                        if (res.ok) {
                                            const data = await res.json();
                                            const shareUrl = `${window.location.origin}/share/${data.share_id}`;
                                            await navigator.clipboard.writeText(shareUrl);
                                            alert('Share link copied to clipboard!');
                                        } else {
                                            alert('Failed to share thread');
                                        }
                                    } catch(e) { alert('Error sharing thread'); }
                                };
                                actions.appendChild(shareBtn);
                                
                                // Delete button
                                const deleteBtn = document.createElement('button');
                                deleteBtn.className = 'thread-action-btn delete';
                                deleteBtn.title = 'Delete';
                                deleteBtn.innerHTML = '■';
                                deleteBtn.onclick = async (e) => {
                                    e.stopPropagation();
                                    if (!confirm('Delete this conversation?')) return;
                                    try {
                                        const res = await fetch(`/api/threads/${t.id}`, { method: 'DELETE' });
                                        if (res.ok) {
                                            loadThreads();
                                        } else {
                                            alert('Failed to delete thread');
                                        }
                                    } catch(e) { alert('Error deleting thread'); }
                                };
                                actions.appendChild(deleteBtn);
                                
                                div.appendChild(actions);
                                list.appendChild(div);
                            });
                        } catch (e) { console.error('Failed to load threads', e); }
                    }

                    document.getElementById('new-chat-btn').onclick = () => {
                        currentThreadId = null;
                        document.getElementById('chat-container').innerHTML = `
                            <div class="welcome-screen">
                                <h1>What do you want to know?</h1>
                                <p>Ask anything. I'll research it for you.</p>
                            </div>
                        `;
                        document.querySelectorAll('.thread-item').forEach(el => el.classList.remove('active'));
                        accumulatedSources = [];
                    };

                    async function loadThread(id) {
                        currentThreadId = id;
                        // Highlight in sidebar
                        document.querySelectorAll('.thread-item').forEach(el => {
                            el.classList.toggle('active', el.dataset.id === id);
                        });

                        const container = document.getElementById('chat-container');
                        container.innerHTML = '<div class="loading">Loading history...</div>';

                        try {
                            const res = await fetch(`/api/threads/${id}/messages`);
                            if (res.status === 401) {
                                window.location.href = '/login';
                                return;
                            }
                            const messages = await res.json();
                            container.innerHTML = '';
                            
                            // Replay messages
                            messages.forEach(msg => appendMessage(msg.role, msg.content));
                            
                            scrollToBottom();
                        } catch (e) {
                            container.innerHTML = '<div class="error">Failed to load thread.</div>';
                        }
                    }

                    // --- Chat Logic ---
                    function appendMessage(role, content) {
                        const container = document.getElementById('chat-container');
                        
                        // Remove welcome screen if present
                        if (container.querySelector('.welcome-screen')) {
                            container.innerHTML = '';
                        }

                        const msgDiv = document.createElement('div');
                        msgDiv.className = `message ${role}`;
                        
                        const roleDiv = document.createElement('div');
                        roleDiv.className = `message-role ${role}`;
                        roleDiv.textContent = role === 'user' ? 'You' : 'W9';
                        
                        const contentDiv = document.createElement('div');
                        contentDiv.className = 'message-content markdown-body';
                        
                        if (role === 'assistant') {
                            contentDiv.innerHTML = content ? renderMarkdown(content) : '<div class="thinking-process"></div>';
                        } else {
                            contentDiv.textContent = content;
                        }

                        msgDiv.appendChild(roleDiv);
                        msgDiv.appendChild(contentDiv);
                        container.appendChild(msgDiv);
                        scrollToBottom();
                        return contentDiv;
                    }

                    function scrollToBottom() {
                        const container = document.getElementById('chat-container');
                        container.scrollTop = container.scrollHeight;
                    }

                    // --- Input Handling ---
                     const input = document.getElementById('user-input');
                    const modelSelect = document.getElementById('model-select');
                    
                    input.addEventListener('keydown', (e) => {
                        if (e.key === 'Enter' && !e.shiftKey) {
                            e.preventDefault();
                            submitQuery();
                        }
                    });
                    
                    document.getElementById('send-btn').onclick = submitQuery;

                    async function submitQuery() {
                        const query = input.value.trim();
                        if (!query) return;
                        
                        input.value = '';
                        input.style.height = 'auto'; // Reset height
                        
                        appendMessage('user', query);
                        const aiContentDiv = appendMessage('assistant', ''); 
                        
                        // Prepare thinking area
                        let thinkingDiv = aiContentDiv.querySelector('.thinking-process');
                        if (!thinkingDiv) {
                            thinkingDiv = document.createElement('div');
                            thinkingDiv.className = 'thinking-process';
                            aiContentDiv.appendChild(thinkingDiv);
                        }
                        thinkingDiv.style.display = 'block';

                        // Actual Answer Container
                        const answerTextDiv = document.createElement('div');
                        answerTextDiv.className = 'answer-text';
                        aiContentDiv.appendChild(answerTextDiv);

                        accumulatedSources = []; 

                        try {
                            const res = await fetch('/api/query/stream', {
                                method: 'POST',
                                headers: { 'Content-Type': 'application/json' },
                                body: JSON.stringify({
                                    query,
                                    web_search_enabled: document.getElementById('web-search-toggle').checked,
                                    search_reasoning_enabled: document.getElementById('search-reasoning-toggle').checked,
                                    model: modelSelect ? modelSelect.value : 'auto',
                                    search_provider: document.getElementById('provider-select').value,
                                    thread_id: currentThreadId 
                                })
                            });

                            if (res.status === 401) {
                                window.location.href = '/login';
                                return;
                            }

                            const reader = res.body.getReader();
                            const decoder = new TextDecoder();
                            let buffer = '';
                            let fullAnswer = '';

                            while (true) {
                                const { done, value } = await reader.read();
                                if (done) break;
                                buffer += decoder.decode(value, { stream: true });
                                const lines = buffer.split('\n\n');
                                buffer = lines.pop();

                                for (const line of lines) {
                                    if (line.startsWith('data: ')) {
                                        try {
                                            const event = JSON.parse(line.substring(6));
                                            
                                            if (event.type === 'Status') {
                                                if (event.data.startsWith('THREAD_ID:')) {
                                                    const newId = event.data.split(':')[1];
                                                    if (!currentThreadId) {
                                                        currentThreadId = newId;
                                                        loadThreads(); 
                                                    }
                                                } else {
                                                    const step = document.createElement('div');
                                                    step.className = 'thinking-step';
                                                    step.textContent = '> ' + event.data;
                                                    thinkingDiv.appendChild(step);
                                                    thinkingDiv.scrollTop = thinkingDiv.scrollHeight;
                                                }
                                            } else if (event.type === 'Source') {
                                                accumulatedSources.push(event.data);
                                            } else if (event.type === 'Answer') {
                                                fullAnswer = event.data;
                                                answerTextDiv.innerHTML = renderMarkdown(fullAnswer);
                                                setTimeout(() => {
                                                    const mermaidDivs = answerTextDiv.querySelectorAll('.mermaid');
                                                    mermaidDivs.forEach(div => {
                                                        if (!div.hasAttribute('data-processed')) {
                                                            mermaid.run({ nodes: [div] });
                                                            div.setAttribute('data-processed', 'true');
                                                        }
                                                    });
                                                }, 50);
                                                scrollToBottom();
                                            } else if (event.type === 'Error') {
                                                answerTextDiv.innerHTML += `<div class="error">${event.data}</div>`;
                                            }
                                        } catch (e) { console.warn(e); }
                                    }
                                }
                            
                            // After done: Show thinking toggle button (collapsible)
                            thinkingDiv.innerHTML = '';
                            thinkingDiv.style.display = 'block';
                            thinkingDiv.style.marginBottom = '10px';
                            
                            // Create toggle button
                            const thinkingToggle = document.createElement('button');
                            thinkingToggle.className = 'thinking-toggle';
                            thinkingToggle.textContent = '💭 Show thinking';
                            thinkingToggle.onclick = () => {
                                const isExpanded = thinkingToggle.textContent.includes('Show');
                                thinkingContent.style.display = isExpanded ? 'block' : 'none';
                                thinkingToggle.textContent = isExpanded ? '💭 Hide thinking' : '💭 Show thinking';
                            };
                            thinkingDiv.appendChild(thinkingToggle);
                            
                            // Create thinking content container (hidden by default)
                            const thinkingContent = document.createElement('div');
                            thinkingContent.className = 'thinking-content';
                            thinkingContent.style.display = 'none';
                            thinkingContent.style.padding = '10px';
                            thinkingContent.style.marginTop = '8px';
                            thinkingContent.style.background = 'rgba(99, 102, 241, 0.1)';
                            thinkingContent.style.borderLeft = '3px solid var(--accent)';
                            thinkingContent.style.fontSize = '0.85em';
                            thinkingContent.style.color = '#888';
                            thinkingContent.style.fontFamily = 'monospace';
                            thinkingContent.style.maxHeight = '200px';
                            thinkingContent.style.overflowY = 'auto';
                            // Move the thinking content to this container
                            const thinkingSteps = thinkingDiv.querySelectorAll('.thinking-step');
                            thinkingSteps.forEach(step => thinkingContent.appendChild(step));
                            thinkingDiv.appendChild(thinkingContent);
                            
                            // Ensure answer is prominent
                            answerTextDiv.style.fontSize = '1.05em';
                            answerTextDiv.style.lineHeight = '1.7';
                            answerTextDiv.style.padding = '12px';
                            answerTextDiv.style.background = 'var(--surface)';
                            answerTextDiv.style.borderRadius = '8px';
                            answerTextDiv.style.border = '1px solid var(--border)';

                        } catch (e) {
                            answerTextDiv.innerHTML += `<div class="error">Error: ${e.message}</div>`;
                            thinkingDiv.style.display = 'none';
                        }
                    }

                    // --- Markdown Renderer ---
                    function renderMarkdown(markdown) {
                        const html = marked.parse(markdown);
                        const tempDiv = document.createElement('div');
                        tempDiv.innerHTML = html;
                        
                        const citationRegex = /\[(\d+)\]/g;
                        
                        function processTextNodes(node) {
                            if (node.nodeType === 3) {
                                const text = node.nodeValue;
                                if (citationRegex.test(text)) {
                                    const fragment = document.createDocumentFragment();
                                    let lastIndex = 0;
                                    text.replace(citationRegex, (match, num, offset) => {
                                        fragment.appendChild(document.createTextNode(text.substring(lastIndex, offset)));
                                        
                                        const span = document.createElement('span');
                                        span.className = 'citation';
                                        span.textContent = `[${num}]`;
                                        
                                        const tooltip = document.createElement('div');
                                        tooltip.className = 'citation-tooltip';
                                        
                                        const source = accumulatedSources[parseInt(num) - 1];
                                        if (source) {
                                            tooltip.innerHTML = `
                                                <span class="citation-tooltip-title">${source.title}</span>
                                                <span class="citation-tooltip-url">${source.url}</span>
                                                <a href="${source.url}" target="_blank" class="citation-link">Open →</a>
                                            `;
                                            span.onclick = (e) => {
                                                e.stopPropagation();
                                                window.open(source.url, '_blank');
                                            };
                                        } else {
                                            tooltip.textContent = `Source ${num}`;
                                        }
                                        
                                        span.appendChild(tooltip);
                                        fragment.appendChild(span);
                                        lastIndex = offset + match.length;
                                    });
                                    fragment.appendChild(document.createTextNode(text.substring(lastIndex)));
                                    node.parentNode.replaceChild(fragment, node);
                                }
                            } else if (node.nodeType === 1 && !['CODE', 'PRE', 'A'].includes(node.tagName)) {
                                Array.from(node.childNodes).forEach(processTextNodes);
                            }
                        }
                        
                        Array.from(tempDiv.childNodes).forEach(processTextNodes);
                        
                        tempDiv.querySelectorAll('code.language-mermaid, pre code.language-mermaid').forEach((block, index) => {
                            const mermaidDiv = document.createElement('div');
                            mermaidDiv.className = 'mermaid';
                            mermaidDiv.id = 'mermaid-' + Date.now() + '-' + index;
                            mermaidDiv.textContent = block.textContent;
                            const pre = block.closest('pre');
                            if (pre) pre.parentNode.replaceChild(mermaidDiv, pre);
                            else block.parentNode.replaceChild(mermaidDiv, block);
                        });
                        
                        return tempDiv.innerHTML;
                    }
                    "#))
                }
            }
        }
    };
    Html(markup.into_string()).into_response()
}

#[derive(Debug, Deserialize)]
pub struct ProviderToggleForm {
    pub enabled: bool,
}

pub async fn admin(headers: HeaderMap, State(state): State<AppState>) -> Response {
    let Some(session) = auth::require_admin(&headers) else {
        return Redirect::to("/login").into_response();
    };

    let statuses = state.db.get_provider_settings().await.unwrap_or_default();
    let metrics = state
        .db
        .get_all_provider_metrics()
        .await
        .unwrap_or_default();
    let models = state.llm_manager.get_models().await;

    let mut model_counts: HashMap<String, usize> = HashMap::new();
    for model in &models {
        *model_counts
            .entry(model.provider.as_str().to_string())
            .or_insert(0) += 1;
    }

    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "W9 Search - Admin" }
                link rel="icon" href="/static/favicon.svg" type="image/svg+xml";
                link rel="stylesheet" href="/static/style.css";
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link href=(r#"https://fonts.googleapis.com/css2?family=Press+Start+2P&family=VT323&display=swap"#) rel="stylesheet";
            }
            body {
                div class="container" {
                    header {
                        img class="brand-banner" src="/static/w9-search-banner.svg" alt="W9 Search";
                        h1 { "Admin Control Panel" }
                        p class="subtitle" { "Enable or disable providers, then refresh the model cache." }
                        nav {
                            a href="/" class="nav-link" { "← Back to Search" }
                            a href="/models" class="nav-link" { "Auto Routing" }
                            a href="/logout" class="nav-link" { "Logout" }
                        }
                    }

                    div class="section" {
                        h2 { "Provider Switches" }
                        div class="grid-container" {
                            @for status in &statuses {
                                @let provider_name = ProviderType::from_str(&status.provider)
                                    .map(|p| p.to_string())
                                    .unwrap_or_else(|| status.provider.clone());
                                @let model_count = model_counts.get(&status.provider).copied().unwrap_or(0);
                                div class="card" {
                                    div class="card-header" {
                                        span class="provider-badge" { (provider_name.clone()) }
                                        @if status.enabled {
                                            span class="badge badge--ok" { "Enabled" }
                                        } @else {
                                            span class="badge badge--err" { "Disabled" }
                                        }
                                    }
                                    div class="card-body" {
                                        div class="meta-item" {
                                            span class="label" { "Loaded models" }
                                            span { (model_count) }
                                        }
                                        div class="meta-item" {
                                            span class="label" { "Status" }
                                            @if status.enabled {
                                                span { "Available for auto routing" }
                                            } @else {
                                                span { "Filtered out of routing" }
                                            }
                                        }
                                        form method="POST" action=(format!("/admin/providers/{}", status.provider)) {
                                            input type="hidden" name="enabled" value=(if status.enabled { "false" } else { "true" });
                                            @if status.enabled {
                                                button type="submit" class="btn" { "Disable" }
                                            } @else {
                                                button type="submit" class="btn" { "Enable" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div class="section" {
                        h2 { "Current Metrics" }
                        div class="grid-container" {
                            @for metric in &metrics {
                                div class="metric-card" {
                                    div class="metric-title" { (metric.provider) }
                                    div class="metric-row" {
                                        span class="metric-name" { "Minute requests" }
                                        span { (metric.req_min.unwrap_or(0)) }
                                    }
                                    div class="metric-row" {
                                        span class="metric-name" { "Daily requests" }
                                        span { (metric.req_day.unwrap_or(0)) }
                                    }
                                    div class="metric-row" {
                                        span class="metric-name" { "Monthly requests" }
                                        span { (metric.req_month.unwrap_or(0)) }
                                    }
                                }
                            }
                        }
                    }

                    div class="section" {
                        h2 { "Live Context" }
                        div class="card" {
                            div class="meta-item" {
                                span class="label" { "Signed in as" }
                                span { (session.email) }
                            }
                            div class="meta-item" {
                                span class="label" { "Role" }
                                span { (session.role) }
                            }
                            p class="text-muted" {
                                "Provider changes take effect after the cache refresh runs, which is triggered automatically after each toggle."
                            }
                        }
                    }
                }
            }
        }
    };

    Html(markup.into_string()).into_response()
}

pub async fn toggle_provider(
    headers: HeaderMap,
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Form(form): Form<ProviderToggleForm>,
) -> Response {
    let Some(_) = auth::require_admin(&headers) else {
        return Redirect::to("/login").into_response();
    };

    let provider_key = provider.to_lowercase();
    if ProviderType::from_str(&provider_key).is_none() {
        return Redirect::to("/admin").into_response();
    }

    if let Err(e) = state
        .db
        .set_provider_enabled(&provider_key, form.enabled)
        .await
    {
        tracing::error!("Failed to update provider {}: {}", provider_key, e);
        return Redirect::to("/admin").into_response();
    }

    if let Err(e) = state.llm_manager.fetch_available_models().await {
        tracing::warn!("Provider toggle refresh failed: {}", e);
    }

    Redirect::to("/admin").into_response()
}
