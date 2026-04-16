use crate::llm::ProviderType;
use crate::models::Source;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool};
use sqlx::FromRow;
use std::collections::HashMap;

#[derive(FromRow)]
struct ProviderMetricsRow {
    req_min: Option<i64>,
    req_day: Option<i64>,
    req_month: Option<i64>,
    last_reset_min: Option<chrono::NaiveDateTime>,
    last_reset_day: Option<chrono::NaiveDateTime>,
    last_reset_month: Option<chrono::NaiveDateTime>,
    limit_min: Option<i64>,
    limit_day: Option<i64>,
    limit_month: Option<i64>,
}

#[derive(FromRow)]
struct ProviderStatusRow {
    provider: String,
    enabled: bool,
}

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(database_url: &str) -> anyhow::Result<Self> {
        tracing::info!("Initializing database connection to: {}", database_url);

        let options = if database_url.starts_with("sqlite:") {
            let path = database_url.strip_prefix("sqlite:").unwrap();
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
        } else {
            database_url
                .parse::<SqliteConnectOptions>()?
                .create_if_missing(true)
        };

        let pool = SqlitePool::connect_with(options).await?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sources (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS provider_metrics (
                provider TEXT PRIMARY KEY,
                req_min INTEGER DEFAULT 0,
                req_day INTEGER DEFAULT 0,
                req_month INTEGER DEFAULT 0,
                last_reset_min DATETIME,
                last_reset_day DATETIME,
                last_reset_month DATETIME
            );

            CREATE TABLE IF NOT EXISTS provider_settings (
                provider TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL DEFAULT 1,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS threads (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                thread_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY(thread_id) REFERENCES threads(id) ON DELETE CASCADE
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Attempt to add limit columns if they don't exist
        let _ = sqlx::query("ALTER TABLE provider_metrics ADD COLUMN limit_min INTEGER")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE provider_metrics ADD COLUMN limit_day INTEGER")
            .execute(&self.pool)
            .await;
        let _ = sqlx::query("ALTER TABLE provider_metrics ADD COLUMN limit_month INTEGER")
            .execute(&self.pool)
            .await;

        for provider in ProviderType::all() {
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO provider_settings (provider, enabled) VALUES (?, 1)",
            )
            .bind(provider.as_str())
            .execute(&self.pool)
            .await;
        }

        Ok(())
    }

    pub async fn create_thread(&self, title: &str) -> anyhow::Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO threads (id, title) VALUES (?, ?)")
            .bind(&id)
            .bind(title)
            .execute(&self.pool)
            .await?;
        Ok(id)
    }

    pub async fn get_thread(&self, id: &str) -> anyhow::Result<Option<crate::models::Thread>> {
        let thread = sqlx::query_as::<_, crate::models::Thread>(
            "SELECT id, title, created_at, updated_at FROM threads WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(thread)
    }

    pub async fn list_threads(&self, limit: i64) -> anyhow::Result<Vec<crate::models::Thread>> {
        let threads = sqlx::query_as::<_, crate::models::Thread>(
            "SELECT id, title, created_at, updated_at FROM threads ORDER BY updated_at DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(threads)
    }

    pub async fn add_message(
        &self,
        thread_id: &str,
        role: &str,
        content: &str,
    ) -> anyhow::Result<i64> {
        // Update thread updated_at
        sqlx::query("UPDATE threads SET updated_at = CURRENT_TIMESTAMP WHERE id = ?")
            .bind(thread_id)
            .execute(&self.pool)
            .await?;

        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO messages (thread_id, role, content) VALUES (?, ?, ?) RETURNING id",
        )
        .bind(thread_id)
        .bind(role)
        .bind(content)
        .fetch_one(&self.pool)
        .await?;
        Ok(id)
    }

    pub async fn get_thread_messages(
        &self,
        thread_id: &str,
    ) -> anyhow::Result<Vec<crate::models::Message>> {
        let messages = sqlx::query_as::<_, crate::models::Message>(
            "SELECT id, thread_id, role, content, created_at FROM messages WHERE thread_id = ? ORDER BY created_at ASC"
        )
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(messages)
    }

    pub async fn insert_source(
        &self,
        url: &str,
        title: &str,
        content: &str,
    ) -> anyhow::Result<i64> {
        let id = sqlx::query_scalar::<_, i64>(
            r#"
            INSERT INTO sources (url, title, content)
            VALUES (?, ?, ?)
            ON CONFLICT(url) DO UPDATE SET title = excluded.title, content = excluded.content
            RETURNING id
            "#,
        )
        .bind(url)
        .bind(title)
        .bind(content)
        .fetch_one(&self.pool)
        .await?;

        Ok(id)
    }

    pub async fn get_sources(&self, limit: i64) -> anyhow::Result<Vec<Source>> {
        let sources = sqlx::query_as::<_, Source>(
            "SELECT id, url, title, content, created_at FROM sources ORDER BY created_at DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(sources)
    }

    pub async fn get_provider_settings(
        &self,
    ) -> anyhow::Result<Vec<crate::models::ProviderStatus>> {
        let rows = sqlx::query_as::<_, ProviderStatusRow>(
            "SELECT provider, enabled FROM provider_settings ORDER BY provider",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| crate::models::ProviderStatus {
                provider: row.provider,
                enabled: row.enabled,
            })
            .collect())
    }

    pub async fn get_provider_status_map(&self) -> anyhow::Result<HashMap<String, bool>> {
        let mut map = HashMap::new();
        for status in self.get_provider_settings().await? {
            map.insert(status.provider, status.enabled);
        }
        Ok(map)
    }

    pub async fn is_provider_enabled(&self, provider: &str) -> anyhow::Result<bool> {
        let row = sqlx::query_scalar::<_, bool>(
            "SELECT enabled FROM provider_settings WHERE provider = ?",
        )
        .bind(provider)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.unwrap_or(true))
    }

    pub async fn set_provider_enabled(&self, provider: &str, enabled: bool) -> anyhow::Result<()> {
        sqlx::query(
            r#"
            INSERT INTO provider_settings (provider, enabled, updated_at)
            VALUES (?, ?, CURRENT_TIMESTAMP)
            ON CONFLICT(provider) DO UPDATE SET
                enabled = excluded.enabled,
                updated_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(provider)
        .bind(enabled)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn search_sources(&self, query: &str, limit: i64) -> anyhow::Result<Vec<Source>> {
        let sources = sqlx::query_as::<_, Source>(
            "SELECT id, url, title, content, created_at FROM sources WHERE content LIKE ? OR title LIKE ? ORDER BY created_at DESC LIMIT ?"
        )
        .bind(format!("%{}%", query))
        .bind(format!("%{}%", query))
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(sources)
    }

    pub async fn update_provider_limits(
        &self,
        provider: &ProviderType,
        requests_remaining_min: Option<i64>,
        requests_remaining_day: Option<i64>,
        limit_min: Option<i64>,
        limit_day: Option<i64>,
    ) -> anyhow::Result<()> {
        let provider_str = provider.as_str();

        let row = sqlx::query_as::<_, ProviderMetricsRow>(
            "SELECT req_min, req_day, req_month, last_reset_min, last_reset_day, last_reset_month, limit_min, limit_day, limit_month FROM provider_metrics WHERE provider = ?"
        )
        .bind(provider_str)
        .fetch_optional(&self.pool)
        .await?;

        let (default_limit_min, default_limit_day, _) = self.get_default_limits(provider);

        let current_limit_min = limit_min
            .or(row.as_ref().and_then(|r| r.limit_min))
            .unwrap_or(default_limit_min);

        let current_limit_day = limit_day
            .or(row.as_ref().and_then(|r| r.limit_day))
            .unwrap_or(default_limit_day);

        let new_req_min = requests_remaining_min.map(|rem| current_limit_min.saturating_sub(rem));
        let new_req_day = requests_remaining_day.map(|rem| current_limit_day.saturating_sub(rem));

        let mut sql = "UPDATE provider_metrics SET ".to_string();
        let mut updates = Vec::new();

        if let Some(val) = new_req_min {
            updates.push(format!("req_min = {}", val));
        }
        if let Some(val) = new_req_day {
            updates.push(format!("req_day = {}", val));
        }
        if let Some(val) = limit_min {
            updates.push(format!("limit_min = {}", val));
        }
        if let Some(val) = limit_day {
            updates.push(format!("limit_day = {}", val));
        }

        if updates.is_empty() {
            return Ok(());
        }

        sql.push_str(&updates.join(", "));
        sql.push_str(" WHERE provider = ?");

        sqlx::query(&sql)
            .bind(provider_str)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    fn get_default_limits(&self, provider: &ProviderType) -> (i64, i64, i64) {
        match provider {
            ProviderType::OpenRouter => (20, 50, 1000000),
            ProviderType::Groq => (30, 14400, 1000000),
            ProviderType::Cerebras => (1000, 1000, 1000000),
            ProviderType::Cohere => (20, 1000000, 1000),
            ProviderType::Pollinations => (1000, 1000, 1000000), // Defaulting to high daily allowance
        }
    }

    pub async fn check_rate_limit(&self, provider: &ProviderType) -> anyhow::Result<bool> {
        let now = Utc::now();
        let provider_str = provider.as_str();

        let row = sqlx::query_as::<_, ProviderMetricsRow>(
            "SELECT req_min, req_day, req_month, last_reset_min, last_reset_day, last_reset_month, limit_min, limit_day, limit_month FROM provider_metrics WHERE provider = ?"
        )
        .bind(provider_str)
        .fetch_optional(&self.pool)
        .await?;

        let (mut req_min, mut req_day, mut req_month) = if let Some(r) = &row {
            (
                r.req_min.unwrap_or(0),
                r.req_day.unwrap_or(0),
                r.req_month.unwrap_or(0),
            )
        } else {
            (0, 0, 0)
        };

        fn to_utc(dt: Option<chrono::NaiveDateTime>) -> DateTime<Utc> {
            match dt {
                Some(t) => Utc.from_utc_datetime(&t),
                None => DateTime::from_timestamp(0, 0).unwrap_or_default(),
            }
        }

        let last_reset_min = row
            .as_ref()
            .map(|r| to_utc(r.last_reset_min))
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap_or_default());
        let last_reset_day = row
            .as_ref()
            .map(|r| to_utc(r.last_reset_day))
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap_or_default());
        let last_reset_month = row
            .as_ref()
            .map(|r| to_utc(r.last_reset_month))
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap_or_default());

        let (def_min, def_day, def_month) = self.get_default_limits(provider);

        let limit_min = row.as_ref().and_then(|r| r.limit_min).unwrap_or(def_min);
        let limit_day = row.as_ref().and_then(|r| r.limit_day).unwrap_or(def_day);
        let limit_month = row
            .as_ref()
            .and_then(|r| r.limit_month)
            .unwrap_or(def_month);

        let mut needs_reset_min = false;
        let mut needs_reset_day = false;
        let mut needs_reset_month = false;

        if now.signed_duration_since(last_reset_min).num_seconds() >= 60 {
            needs_reset_min = true;
        }

        match provider {
            ProviderType::OpenRouter | ProviderType::Pollinations => {
                if now.date_naive() > last_reset_day.date_naive() {
                    needs_reset_day = true;
                }
            }
            ProviderType::Groq | ProviderType::Cerebras => {
                if now.signed_duration_since(last_reset_day).num_hours() >= 24 {
                    needs_reset_day = true;
                }
            }
            _ => {
                if now.date_naive() > last_reset_day.date_naive() {
                    needs_reset_day = true;
                }
            }
        }

        if provider == &ProviderType::Cohere {
            if now.month() != last_reset_month.month() || now.year() != last_reset_month.year() {
                needs_reset_month = true;
            }
        }

        if needs_reset_min {
            req_min = 0;
        }
        if needs_reset_day {
            req_day = 0;
        }
        if needs_reset_month {
            req_month = 0;
        }

        if req_min >= limit_min {
            tracing::warn!(
                "Rate limit exceeded for {} (Minute): {}/{}",
                provider_str,
                req_min,
                limit_min
            );
            return Ok(false);
        }
        if req_day >= limit_day {
            tracing::warn!(
                "Rate limit exceeded for {} (Day): {}/{}",
                provider_str,
                req_day,
                limit_day
            );
            return Ok(false);
        }
        if req_month >= limit_month {
            tracing::warn!(
                "Rate limit exceeded for {} (Month): {}/{}",
                provider_str,
                req_month,
                limit_month
            );
            return Ok(false);
        }

        req_min += 1;
        req_day += 1;
        req_month += 1;

        let new_reset_min = if needs_reset_min { now } else { last_reset_min };
        let new_reset_day = if needs_reset_day { now } else { last_reset_day };
        let new_reset_month = if needs_reset_month {
            now
        } else {
            last_reset_month
        };

        sqlx::query(
            r#"
            INSERT INTO provider_metrics (
                provider, req_min, req_day, req_month, 
                last_reset_min, last_reset_day, last_reset_month,
                limit_min, limit_day, limit_month
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(provider) DO UPDATE SET
                req_min = excluded.req_min,
                req_day = excluded.req_day,
                req_month = excluded.req_month,
                last_reset_min = excluded.last_reset_min,
                last_reset_day = excluded.last_reset_day,
                last_reset_month = excluded.last_reset_month,
                limit_min = coalesce(excluded.limit_min, provider_metrics.limit_min),
                limit_day = coalesce(excluded.limit_day, provider_metrics.limit_day),
                limit_month = coalesce(excluded.limit_month, provider_metrics.limit_month)
            "#,
        )
        .bind(provider_str)
        .bind(req_min)
        .bind(req_day)
        .bind(req_month)
        .bind(new_reset_min)
        .bind(new_reset_day)
        .bind(new_reset_month)
        .bind(limit_min)
        .bind(limit_day)
        .bind(limit_month)
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    pub async fn update_search_limits(
        &self,
        provider_name: &str,
        used_month: Option<i64>,
        limit_month: Option<i64>,
        used_min: Option<i64>,
    ) -> anyhow::Result<()> {
        let mut sql = "UPDATE provider_metrics SET ".to_string();
        let mut updates = Vec::new();

        if let Some(val) = used_month {
            updates.push(format!("req_month = {}", val));
        }
        if let Some(val) = limit_month {
            updates.push(format!("limit_month = {}", val));
        }
        if let Some(val) = used_min {
            updates.push(format!("req_min = {}", val));
        }

        // Also update timestamp to now to prevent immediate local reset if we just got fresh data
        updates.push("last_reset_month = CASE WHEN last_reset_month IS NULL THEN CURRENT_TIMESTAMP ELSE last_reset_month END".to_string());
        // Actually, if we get fresh usage from API, we trust it. But our local reset logic relies on timestamps.
        // If API says "100 used this month", we set req_month=100.
        // If we don't update last_reset_month, our local logic might reset it to 0 if it thinks month changed.
        // But we shouldn't change last_reset_month unless we know it's a new month.
        // Let's just update the counters.

        if updates.is_empty() {
            return Ok(());
        }

        sql.push_str(&updates.join(", "));
        sql.push_str(" WHERE provider = ?");

        sqlx::query(&sql)
            .bind(provider_name)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn check_search_rate_limit(
        &self,
        provider_name: &str,
        cost: i64,
    ) -> anyhow::Result<bool> {
        let now = Utc::now();

        let row = sqlx::query_as::<_, ProviderMetricsRow>(
            "SELECT req_min, req_day, req_month, last_reset_min, last_reset_day, last_reset_month, limit_min, limit_day, limit_month FROM provider_metrics WHERE provider = ?"
        )
        .bind(provider_name)
        .fetch_optional(&self.pool)
        .await?;

        let (mut req_min, mut req_day, mut req_month) = if let Some(r) = &row {
            (
                r.req_min.unwrap_or(0),
                r.req_day.unwrap_or(0),
                r.req_month.unwrap_or(0),
            )
        } else {
            (0, 0, 0)
        };

        fn to_utc(dt: Option<chrono::NaiveDateTime>) -> DateTime<Utc> {
            match dt {
                Some(t) => Utc.from_utc_datetime(&t),
                None => DateTime::from_timestamp(0, 0).unwrap_or_default(),
            }
        }

        let last_reset_min = row
            .as_ref()
            .map(|r| to_utc(r.last_reset_min))
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap_or_default());
        let last_reset_month = row
            .as_ref()
            .map(|r| to_utc(r.last_reset_month))
            .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap_or_default());

        // Define Limits
        // Brave: 1 req/sec (approx 60/min), 2000/month
        // Tavily: 1000/month
        let (limit_min, limit_month) = match provider_name {
            "search:brave" => (60, 2000),
            "search:tavily" => (1000000, 1000), // No minute limit specified for Tavily, just monthly credits
            _ => (1000000, 1000000),
        };

        // Reset Logic
        let mut needs_reset_min = false;
        let mut needs_reset_month = false;

        // Minute reset (rolling 60s)
        if now.signed_duration_since(last_reset_min).num_seconds() >= 60 {
            needs_reset_min = true;
        }

        // Month Reset (1st of month)
        if now.month() != last_reset_month.month() || now.year() != last_reset_month.year() {
            needs_reset_month = true;
        }

        if needs_reset_min {
            req_min = 0;
        }
        if needs_reset_month {
            req_month = 0;
        }

        // Check Limits
        if req_min + cost > limit_min {
            tracing::warn!(
                "Search rate limit exceeded for {} (Minute): {}/{}",
                provider_name,
                req_min,
                limit_min
            );
            return Ok(false);
        }
        if req_month + cost > limit_month {
            tracing::warn!(
                "Search rate limit exceeded for {} (Month): {}/{}",
                provider_name,
                req_month,
                limit_month
            );
            return Ok(false);
        }

        req_min += cost;
        req_month += cost;

        let new_reset_min = if needs_reset_min { now } else { last_reset_min };
        let new_reset_month = if needs_reset_month {
            now
        } else {
            last_reset_month
        };
        // We don't track daily for search currently, just reuse existing column with no updates or dummy
        let new_reset_day = row
            .as_ref()
            .map(|r| to_utc(r.last_reset_day))
            .unwrap_or(now);

        sqlx::query(
            r#"
            INSERT INTO provider_metrics (
                provider, req_min, req_day, req_month, 
                last_reset_min, last_reset_day, last_reset_month,
                limit_min, limit_day, limit_month
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(provider) DO UPDATE SET
                req_min = excluded.req_min,
                req_month = excluded.req_month,
                last_reset_min = excluded.last_reset_min,
                last_reset_month = excluded.last_reset_month
            "#,
        )
        .bind(provider_name)
        .bind(req_min)
        .bind(req_day) // Keep existing day count
        .bind(req_month)
        .bind(new_reset_min)
        .bind(new_reset_day)
        .bind(new_reset_month)
        .bind(limit_min)
        .bind(0) // Dummy daily limit
        .bind(limit_month)
        .execute(&self.pool)
        .await?;

        Ok(true)
    }

    pub async fn get_all_provider_metrics(
        &self,
    ) -> anyhow::Result<Vec<crate::models::ProviderMetrics>> {
        let metrics = sqlx::query_as::<_, crate::models::ProviderMetrics>(
            "SELECT provider, req_min, req_day, req_month, limit_min, limit_day, limit_month FROM provider_metrics"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(metrics)
    }
}
