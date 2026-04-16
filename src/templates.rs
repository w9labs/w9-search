use axum::{extract::State, response::Html};
use maud::{html, Markup, DOCTYPE};
use crate::AppState;

pub async fn models(State(state): State<AppState>) -> Html<String> {
    // Fetch models and limits
    let mut models = state.llm_manager.get_models().await;
    
    // Sort models by provider, then name
    models.sort_by(|a, b| {
        let provider_cmp = a.provider.as_str().cmp(b.provider.as_str());
        if provider_cmp == std::cmp::Ordering::Equal {
            a.name.cmp(&b.name)
        } else {
            provider_cmp
        }
    });

    let metrics = state.db.get_all_provider_metrics().await.unwrap_or_default();

    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "W9 Search - Models & Limits" }
                link rel="stylesheet" href="/static/style.css";
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link href=(r#"https://fonts.googleapis.com/css2?family=Press+Start+2P&family=VT323&display=swap"#) rel="stylesheet";
            }
            body {
                div class="container" {
                    header {
                        h1 { "W9 Search" }
                        p class="subtitle" { "Models & Limits" }
                        nav {
                            a href="/" class="nav-link" { "← Back to Search" }
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
                        h2 { "Available Models" }
                        div class="grid-container" {
                            @for model in &models {
                                div class="card" {
                                    div class="card-header" {
                                        span class="provider-badge" { (model.provider) }
                                        h3 { (model.name) }
                                    }
                                    div class="card-body" {
                                        div class="meta-item" {
                                            span class="label" { "ID:" }
                                            code { (model.id) }
                                        }
                                        div class="meta-item" {
                                            span class="label" { "Context:" }
                                            span { (model.context_length.map(|c| c.to_string()).unwrap_or("Unknown".to_string())) }
                                        }
                                        div class="meta-item" {
                                            span class="label" { "Access:" }
                                            span class=(if model.is_free { "tag-free" } else { "tag-paid" }) {
                                                (if model.is_free { "Free" } else { "Paid" })
                                            }
                                        }
                                        @if let Some(description) = &model.description {
                                            div class="meta-item model-description" {
                                                span class="label" { "About:" }
                                                span { (description) }
                                            }
                                        }
                                        div class="model-badges" {
                                            @if model.supports_native_search {
                                                span class="badge badge--ok" { "Search" }
                                            }
                                            @if model.supports_reasoning {
                                                span class="badge badge--warn" { "Reasoning" }
                                            }
                                            @if model.supports_tools {
                                                span class="badge badge--warn" { "Tools" }
                                            }
                                            @if model.is_specialized {
                                                span class="badge badge--err" { "Specialized" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    Html(markup.into_string())
}

pub async fn index(State(state): State<AppState>) -> Html<String> {
    // Fetch models dynamically from LLMManager
    let mut models = state.llm_manager.get_models().await;
    
    // Sort models by provider, then name
    models.sort_by(|a, b| {
        let provider_cmp = a.provider.as_str().cmp(b.provider.as_str());
        if provider_cmp == std::cmp::Ordering::Equal {
            a.name.cmp(&b.name)
        } else {
            provider_cmp
        }
    });

    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "W9 Search" }
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
                        div class="logo" { "W9 SEARCH" }
                        button id="new-chat-btn" class="new-chat-btn" title="New Chat" { "+" }
                    }
                    div id="thread-list" class="thread-list" {
                        // Threads will be injected here
                    }
                    div class="sidebar-footer" {
                        a href="/models" class="nav-link" { "Models & Limits" }
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
                            div class="control-group" {
                                select id="model-select" {
                                    option value="auto" { "Auto (Smart)" }
                                    @for model in &models {
                                        option value=(model.id) {
                                            (format!(
                                                "{} ({}){}{}{}{}",
                                                model.name,
                                                model.provider,
                                                if model.supports_native_search { " [search]" } else { "" },
                                                if model.supports_reasoning { " [reasoning]" } else { "" },
                                                if model.supports_tools { " [tools]" } else { "" },
                                                if model.is_specialized { " [safety]" } else { "" }
                                            ))
                                        }
                                    }
                                }
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
                            "Native-search models skip local web search. The reasoning toggle expands the planner for models that need it."
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
                            const threads = await res.json();
                            const list = document.getElementById('thread-list');
                            list.innerHTML = '';
                            threads.forEach(t => {
                                const div = document.createElement('div');
                                div.className = 'thread-item';
                                div.textContent = t.title || 'Untitled Chat';
                                div.dataset.id = t.id;
                                div.onclick = () => loadThread(t.id);
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
                                    model: document.getElementById('model-select').value,
                                    search_provider: document.getElementById('provider-select').value,
                                    thread_id: currentThreadId 
                                })
                            });

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
                            }
                            
                            // Collapse thinking after done
                            thinkingDiv.style.display = 'none'; // Auto-hide thinking process
                            
                        } catch (e) {
                            answerTextDiv.innerHTML += `<div class="error">Error: ${e.message}</div>`;
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
    Html(markup.into_string())
}
