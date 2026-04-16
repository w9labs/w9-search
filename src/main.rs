mod api;
mod auth;
mod db;
mod llm;
mod models;
mod rag;
mod search;
mod templates;
mod tools;

use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::db::Database;
use crate::llm::LLMManager;
use crate::search::WebSearch;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub llm_manager: Arc<LLMManager>,
}

#[tokio::main]
async fn main() {
    // Initialize logging first - ensure it writes to stderr for Docker logs
    let log_level = std::env::var("RUST_LOG")
        .unwrap_or_else(|_| "info".to_string())
        .parse::<tracing::Level>()
        .unwrap_or(tracing::Level::INFO);

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(true)
        .with_writer(std::io::stderr)
        .with_ansi(false) // Disable ANSI colors for Docker logs
        .init();

    // Set panic hook to log panics with full backtrace
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        eprintln!("═══════════════════════════════════════════════════════════");
        eprintln!("PANIC OCCURRED!");
        eprintln!("═══════════════════════════════════════════════════════════");
        eprintln!("Location: {:?}", panic_info.location());
        eprintln!("Message: {:?}", panic_info.payload().downcast_ref::<&str>());
        eprintln!("Backtrace:\n{}", backtrace);
        eprintln!("═══════════════════════════════════════════════════════════");
        tracing::error!("PANIC: {:?}", panic_info);
        tracing::error!("Backtrace: {}", backtrace);
    }));

    // Load .env file (ignore errors if it doesn't exist)
    dotenv::dotenv().ok();

    eprintln!("=== W9 Search Starting ===");
    tracing::info!("=== W9 Search Starting ===");

    if let Err(e) = run().await {
        eprintln!("=== FATAL ERROR ===");
        eprintln!("Fatal error: {}", e);
        eprintln!("Error chain: {:?}", e);
        eprintln!("==================");
        tracing::error!("Fatal error: {}", e);
        tracing::error!("Error chain: {:?}", e);

        // Flush logs before exiting
        std::io::Write::flush(&mut std::io::stderr()).ok();
        std::process::exit(1);
    }
}

async fn run() -> anyhow::Result<()> {
    eprintln!("Starting W9 Search application...");
    tracing::info!("Starting W9 Search application...");

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:/app/data/w9_search.db".to_string());

    tracing::info!("Database URL: {}", database_url);

    // Ensure database directory exists if path contains directories
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        let db_path = std::path::Path::new(path);

        if let Some(parent) = db_path.parent() {
            // Only create directory if parent is not empty (i.e., path contains directories)
            if !parent.as_os_str().is_empty() {
                tracing::info!("Creating database directory: {:?}", parent);
                std::fs::create_dir_all(parent)?;

                // Verify directory is writable
                let metadata = std::fs::metadata(parent)?;
                tracing::info!("Directory permissions: {:?}", metadata.permissions());

                // Test write access by creating a temp file
                let test_file = parent.join(".write_test");
                match std::fs::File::create(&test_file) {
                    Ok(_) => {
                        std::fs::remove_file(&test_file).ok();
                        tracing::info!("Directory is writable");
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Database directory {:?} appears not to be writable: {}. \
                            SQLite might fail to open the database.",
                            parent,
                            e
                        );
                    }
                }
            } else {
                tracing::info!(
                    "Database file is in current directory, no parent directory to create"
                );
            }
        }

        // Also try to create an empty database file to ensure the path is accessible
        tracing::info!("Database file path: {:?}", db_path);
        if !db_path.exists() {
            tracing::info!("Database file does not exist, SQLite will create it");
        } else {
            tracing::info!("Database file already exists");
        }
    }

    tracing::info!("Connecting to database...");
    let db = Arc::new(Database::new(&database_url).await?);
    tracing::info!("Database connected successfully");

    tracing::info!("Running database migrations...");
    let mut retry_count = 0;
    while retry_count < 5 {
        match db.migrate().await {
            Ok(_) => {
                tracing::info!("Database migrations completed");
                break;
            }
            Err(e) => {
                retry_count += 1;
                tracing::warn!("Migration failed (attempt {}/5): {}", retry_count, e);
                if retry_count >= 5 {
                    return Err(e);
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }

    // Initialize LLM Manager
    let llm_manager = Arc::new(LLMManager::new(db.clone()));

    // Start background initialization task
    // We do this in the background so the server can start up and pass health checks immediately
    // even if external APIs are slow or timing out.
    let manager_clone = llm_manager.clone();
    let db_clone = db.clone();
    tokio::spawn(async move {
        tracing::info!("Background init: Fetching available models...");
        if let Err(e) = manager_clone.fetch_available_models().await {
            tracing::error!("Background init: Failed to fetch models: {}", e);
        }

        tracing::info!("Background init: Syncing Tavily usage...");
        if let Err(e) = WebSearch::sync_tavily_usage(&db_clone).await {
            tracing::error!("Background init: Failed to sync Tavily usage: {}", e);
        }

        tracing::info!("Background init: Completed");
    });

    let state = AppState { db, llm_manager };

    // Check if static directory exists
    if !std::path::Path::new("static").exists() {
        tracing::warn!("Static directory not found, creating it...");
        std::fs::create_dir_all("static")?;
    }

    // Health check endpoint
    async fn health_check() -> &'static str {
        "OK"
    }

    let app = Router::new()
        .route("/", get(templates::index))
        .route("/login", get(auth::login))
        .route("/oauth/callback", get(auth::callback))
        .route("/logout", get(auth::logout))
        .route("/admin", get(templates::admin))
        .route(
            "/admin/providers/:provider",
            post(templates::toggle_provider),
        )
        .route("/models", get(templates::models))
        .route("/health", get(health_check))
        .route("/api/query", post(api::handle_query))
        .route("/api/query/stream", post(api::handle_query_stream))
        .route("/api/sources", get(api::get_sources))
        .route("/api/sync", post(api::sync_limits))
        .route("/api/threads", get(api::get_threads))
        .route("/api/threads/:id/messages", get(api::get_thread_messages))
        .nest_service("/static", ServeDir::new("static"))
        .layer(CorsLayer::permissive())
        .with_state(state);

    tracing::info!(
        "Router configured with routes: /, /admin, /health, /api/query, /api/sources, /static"
    );

    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .unwrap_or(3000);

    eprintln!("Binding to 0.0.0.0:{}...", port);
    tracing::info!("Binding to 0.0.0.0:{}...", port);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .map_err(|e| {
            eprintln!("Failed to bind to 0.0.0.0:{}: {}", port, e);
            anyhow::anyhow!("Failed to bind to port {}: {}", port, e)
        })?;

    eprintln!("Server listening on http://0.0.0.0:{}", port);
    eprintln!("Application ready to accept connections");
    tracing::info!("Server listening on http://0.0.0.0:{}", port);
    tracing::info!("Application ready to accept connections");

    // Flush stderr to ensure logs are visible
    std::io::Write::flush(&mut std::io::stderr()).ok();

    tracing::info!("Starting Axum server...");
    eprintln!("Starting Axum server...");
    eprintln!("Server will run until interrupted (CTRL+C)");

    // Use a signal handler to gracefully shutdown
    let shutdown = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        tracing::info!("Received shutdown signal");
        eprintln!("Received shutdown signal");
    };

    // Start server with error handling
    match axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
    {
        Ok(_) => {
            tracing::info!("Server shutdown gracefully");
            eprintln!("Server shutdown gracefully");
            Ok(())
        }
        Err(e) => {
            eprintln!("═══════════════════════════════════════════════════════════");
            eprintln!("SERVER ERROR!");
            eprintln!("═══════════════════════════════════════════════════════════");
            eprintln!("Error: {}", e);
            eprintln!("═══════════════════════════════════════════════════════════");
            tracing::error!("Server error: {}", e);
            tracing::error!("Error details: {:?}", e);
            std::io::Write::flush(&mut std::io::stderr()).ok();
            Err(anyhow::anyhow!("Server error: {}", e))
        }
    }
}
