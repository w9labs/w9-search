use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures::StreamExt;
use std::convert::Infallible;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::auth;
use crate::models::{QueryRequest, QueryResponse};
use crate::rag::{RAGSystem, StreamEvent};
use crate::search::WebSearch;
use crate::AppState;

async fn resolve_model(
    state: &AppState,
    requested_model: Option<String>,
    prefer_search: bool,
) -> anyhow::Result<crate::llm::Model> {
    let requested_model = requested_model.unwrap_or_else(|| "auto".to_string());

    if requested_model == "auto" {
        return state
            .llm_manager
            .pick_default_model(prefer_search)
            .await
            .ok_or_else(|| anyhow::anyhow!("No models are loaded yet. Please retry."));
    }

    if let Some(model) = state.llm_manager.get_model(&requested_model).await {
        return Ok(model);
    }

    tracing::warn!(
        "Requested model '{}' not found; falling back to auto selection",
        requested_model
    );

    state
        .llm_manager
        .pick_default_model(prefer_search)
        .await
        .ok_or_else(|| anyhow::anyhow!("No models are loaded yet. Please retry."))
}

pub async fn handle_query_stream(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<QueryRequest>,
) -> Response {
    let session = match auth::require_session(&headers) {
        Some(session) => session,
        None => {
            return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
        }
    };

    let user_email = session.email.clone();

    tracing::info!(
        "Received streaming query: '{}' (web_search: {}, model: {:?}, thread: {:?})",
        request.query,
        request.web_search_enabled,
        request.model,
        request.thread_id
    );

    let (tx, rx) = mpsc::channel(100);

    // Spawn background task to run the query
    tokio::spawn(async move {
        // 1. Thread Management
        let thread_id = match request.thread_id {
            Some(id) => id,
            None => {
                match state.db.create_thread(&request.query, &user_email).await {
                    Ok(id) => {
                        let _ = tx
                            .send(Ok(StreamEvent::Status(format!(
                                "Created new thread: {}",
                                id
                            ))))
                            .await;
                        // Send thread ID to client so it can update URL
                        // We'll define a new event type for this later or just use Status/a specific event
                        let _ = tx
                            .send(Ok(StreamEvent::Status(format!("THREAD_ID:{}", id))))
                            .await;
                        id
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Ok(StreamEvent::Error(format!(
                                "Failed to create thread: {}",
                                e
                            ))))
                            .await;
                        return;
                    }
                }
            }
        };

        // 2. Fetch History
        let history = match state.db.get_thread_messages(&thread_id).await {
            Ok(msgs) => msgs,
            Err(e) => {
                tracing::warn!("Failed to fetch history: {}", e);
                Vec::new()
            }
        };

        // 3. Save User Message
        if let Err(e) = state
            .db
            .add_message(&thread_id, "user", &request.query)
            .await
        {
            tracing::error!("Failed to save user message: {}", e);
        }

        // 4. Model Selection
        let prefer_search = request.web_search_enabled || request.search_reasoning_enabled;
        let requested_model = if auth::can_choose_model_role(&session.role) {
            request.model.clone()
        } else {
            Some("auto".to_string())
        };
        let model = match resolve_model(&state, requested_model, prefer_search).await {
            Ok(model) => model,
            Err(e) => {
                let _ = tx.send(Ok(StreamEvent::Error(e.to_string()))).await;
                let _ = tx.send(Ok(StreamEvent::Done)).await;
                return;
            }
        };

        let search_provider = request.search_provider.filter(|s| s != "auto");

        tracing::info!(
            "Using model '{}' (search: {}, reasoning: {}, tools: {}) and search provider '{:?}'",
            model.id,
            model.supports_native_search,
            model.supports_reasoning,
            model.supports_tools,
            search_provider
        );
        let _ = tx
            .send(Ok(StreamEvent::Status(format!(
                "Using model: {}",
                model.name
            ))))
            .await;

        let rag = RAGSystem::new(
            state.db.clone(),
            state.llm_manager.clone(),
            model,
            search_provider,
        );

        // 5. Execute RAG with history
        match rag
            .query(
                &request.query,
                request.web_search_enabled,
                request.search_reasoning_enabled,
                history,
                Some(tx.clone()),
            )
            .await
        {
            Ok((answer, _)) => {
                let _ = tx.send(Ok(StreamEvent::Answer(answer.clone()))).await;
                // 6. Save Assistant Message
                if let Err(e) = state.db.add_message(&thread_id, "assistant", &answer).await {
                    tracing::error!("Failed to save assistant message: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Query error: {}", e);
                let _ = tx.send(Ok(StreamEvent::Error(e.to_string()))).await;
            }
        }

        let _ = tx.send(Ok(StreamEvent::Done)).await;
    });

    // Create stream from channel
    let stream = ReceiverStream::new(rx).map(|result| match result {
        Ok(event) => Ok::<Event, Infallible>(
            Event::default()
                .json_data(event)
                .unwrap_or_else(|_| Event::default().data("Serialization error")),
        ),
        Err(_) => Ok::<Event, Infallible>(
            Event::default()
                .event("error")
                .data("Internal channel error"),
        ),
    });

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(10)))
        .into_response()
}

pub async fn handle_query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, impl IntoResponse> {
    let session = match auth::require_session(&headers) {
        Some(session) => session,
        None => return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string())),
    };

    // Non-streaming endpoint (legacy support, simplified)
    tracing::info!("Received query: '{}'", request.query);

    let prefer_search = request.web_search_enabled || request.search_reasoning_enabled;
    let requested_model = if auth::can_choose_model_role(&session.role) {
        request.model.clone()
    } else {
        Some("auto".to_string())
    };
    let model = match resolve_model(&state, requested_model, prefer_search).await {
        Ok(model) => model,
        Err(e) => return Err((StatusCode::SERVICE_UNAVAILABLE, e.to_string())),
    };

    let search_provider = request.search_provider.filter(|s| s != "auto");
    let rag = RAGSystem::new(
        state.db.clone(),
        state.llm_manager.clone(),
        model,
        search_provider,
    );

    // For simple query, we don't support history yet
    match rag
        .query(
            &request.query,
            request.web_search_enabled,
            request.search_reasoning_enabled,
            Vec::new(),
            None,
        )
        .await
    {
        Ok((answer, sources)) => Ok(Json(QueryResponse { answer, sources })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))),
    }
}

pub async fn get_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::models::Thread>>, impl IntoResponse> {
    let session = match auth::require_session(&headers) {
        Some(session) => session,
        None => return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string())),
    };

    match state.db.list_threads(&session.email, 50).await {
        Ok(threads) => Ok(Json(threads)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))),
    }
}

pub async fn get_thread_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(thread_id): axum::extract::Path<String>,
) -> Result<Json<Vec<crate::models::Message>>, impl IntoResponse> {
    let session = match auth::require_session(&headers) {
        Some(session) => session,
        None => return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string())),
    };

    let thread = match state.db.get_thread(&thread_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return Err((StatusCode::NOT_FOUND, "Thread not found".to_string())),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))),
    };

    if thread.user_email != session.email {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    match state.db.get_thread_messages(&thread_id).await {
        Ok(messages) => Ok(Json(messages)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))),
    }
}

pub async fn get_sources(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::models::Source>>, impl IntoResponse> {
    if auth::require_session(&headers).is_none() {
        return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string()));
    }

    match state.db.get_sources(20).await {
        Ok(sources) => Ok(Json(sources)),
        Err(e) => {
            tracing::error!("Get sources error: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e)))
        }
    }
}

pub async fn sync_limits(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    if auth::require_session(&headers).is_none() {
        return StatusCode::UNAUTHORIZED;
    }

    // Sync Tavily
    if let Err(e) = WebSearch::sync_tavily_usage(&state.db).await {
        tracing::error!("Sync Tavily limits error: {}", e);
    }

    // Sync LLM Providers (OpenRouter, Pollinations, etc.)
    if let Err(e) = state.llm_manager.refresh_llm_limits().await {
        tracing::error!("Sync LLM limits error: {}", e);
    }

    StatusCode::OK
}

pub async fn delete_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(thread_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let session = match auth::require_session(&headers) {
        Some(session) => session,
        None => return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string())),
    };

    let thread = match state.db.get_thread(&thread_id).await {
        Ok(Some(t)) => t,
        Ok(None) => return Err((StatusCode::NOT_FOUND, "Thread not found".to_string())),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))),
    };

    if thread.user_email != session.email {
        return Err((StatusCode::FORBIDDEN, "Access denied".to_string()));
    }

    match state.db.delete_thread(&thread_id, &session.email).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err((StatusCode::NOT_FOUND, "Thread not found".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Error: {}", e))),
    }
}

#[derive(serde::Serialize)]
pub struct ShareResponse {
    pub share_id: String,
}

pub async fn share_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Path(thread_id): axum::extract::Path<String>,
) -> Result<Json<ShareResponse>, impl IntoResponse> {
    let session = match auth::require_session(&headers) {
        Some(session) => session,
        None => return Err((StatusCode::UNAUTHORIZED, "Unauthorized".to_string())),
    };

    match state.db.share_thread(&thread_id, &session.email).await {
        Ok(share_id) => Ok(Json(ShareResponse { share_id })),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

pub async fn view_shared(
    State(state): State<AppState>,
    axum::extract::Path(share_id): axum::extract::Path<String>,
) -> Result<Json<serde_json::Value>, impl IntoResponse> {
    match state.db.get_shared_thread(&share_id).await {
        Ok(Some((thread_id, messages))) => Ok(Json(serde_json::json!({
            "thread_id": thread_id,
            "messages": messages
        }))),
        Ok(None) => Err((StatusCode::NOT_FOUND, "Shared thread not found".to_string())),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}
