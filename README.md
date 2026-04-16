# W9 Search - AI RAG Web Application

A web application built with the **MASH** stack (Maud + Axum + SQLx + HTMX) that provides AI-powered search with Retrieval Augmented Generation (RAG) capabilities.

## Features

- 🤖 AI-powered answers using OpenRouter plus the four allowed Pollinations models
- 🔍 Hosted SearXNG research path with agentic fallback for models without native search
- 🧠 Optional reasoning mode for deeper search planning
- 📚 RAG system that retrieves and uses web sources when the selected model needs them
- 💾 SQLite database for storing sources
- 🎨 Shared W9 voxel-style UI with mobile-friendly layout

## Tech Stack

- **Maud**: HTML templating
- **Axum**: Web framework
- **SQLx**: Database toolkit
- **HTMX**: Dynamic HTML interactions
- **OpenRouter**: AI model API

## Setup

1. Clone the repository and navigate to the project directory.

2. Create a `.env` file:
```bash
cp .env.example .env
```

3. Add your API keys to `.env` (OpenRouter is required for non-Pollinations models; hosted SearXNG is the default):
```
OPENROUTER_API_KEY=your_key_here
SEARXNG_BASE_URL=https://searxng.w9.nu
```

4. Build and run:
```bash
cargo run
```

5. Open your browser to `http://localhost:3000`
   Production: `https://search.w9.nu`

## Usage

1. Enter your query in the text area
2. Toggle "Web Search" and optional "Reasoning" to control research depth
3. Click "Query" to get AI-powered answers with source citations
4. Sources are automatically stored in the database for future queries

## Project Structure

```
src/
├── main.rs       # Application entry point
├── api.rs        # API handlers
├── db.rs         # Database operations
├── models.rs     # Data models
├── rag.rs        # RAG system implementation
├── search.rs     # Web search functionality
└── templates.rs  # Maud HTML templates
```

## License

MIT
