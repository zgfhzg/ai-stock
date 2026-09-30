use axum::{http::StatusCode, Json};
use feed_rs::parser;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    task::JoinHandle,
    time::{sleep, Duration},
};

use crate::{
    error::{api_error, ApiResult},
    state::AppState,
    strategy::{self, NewsArticleInput},
};

#[derive(Clone, Serialize)]
pub struct NewsArticle {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub published_at_unix: i64,
    pub collected_at_unix: i64,
    pub summary: Option<String>,
    pub analysis_status: String,
    pub ai_summary: Option<String>,
    pub sentiment: Option<String>,
    pub sentiment_score: Option<f64>,
    pub importance: Option<u8>,
    pub impact_horizon: Option<String>,
    pub related_stocks: Vec<Value>,
    pub rationale: Option<String>,
    pub analysis_model: Option<String>,
    pub analyzed_at_unix: Option<i64>,
    pub analysis_error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct NewsAnalysisRun {
    pub requested: usize,
    pub analyzed: usize,
    pub failed: usize,
    pub model: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct NewsCollectorStatus {
    pub running: bool,
    pub collecting: bool,
    pub interval_hours: u64,
    pub retention_days: u64,
    pub daily_limit: u32,
    pub article_count: u64,
    pub database_bytes: u64,
    pub max_database_bytes: u64,
    pub last_started_at_unix: Option<u64>,
    pub last_finished_at_unix: Option<u64>,
    pub next_collection_at_unix: Option<u64>,
    pub last_inserted_count: u32,
    pub last_error: Option<String>,
    pub analyzed_count: u64,
    pub pending_analysis_count: u64,
    pub failed_analysis_count: u64,
    pub last_analysis_error: Option<String>,
    pub analysis_enabled: bool,
}

pub struct NewsCollectorRuntime {
    handle: Option<JoinHandle<()>>,
    status: NewsCollectorStatus,
}

impl Default for NewsCollectorRuntime {
    fn default() -> Self {
        Self {
            handle: None,
            status: NewsCollectorStatus {
                running: false,
                collecting: false,
                interval_hours: 24,
                retention_days: 90,
                daily_limit: 300,
                article_count: 0,
                database_bytes: 0,
                max_database_bytes: 1_073_741_824,
                last_started_at_unix: None,
                last_finished_at_unix: None,
                next_collection_at_unix: None,
                last_inserted_count: 0,
                last_error: None,
                analyzed_count: 0,
                pending_analysis_count: 0,
                failed_analysis_count: 0,
                last_analysis_error: None,
                analysis_enabled: false,
            },
        }
    }
}

pub fn initialize(state: &AppState) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(&state.config.news_db_path).parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = open_database(state)?;
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         CREATE TABLE IF NOT EXISTS news_articles (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             title TEXT NOT NULL,
             url TEXT NOT NULL UNIQUE,
             source TEXT NOT NULL,
             published_at_unix INTEGER NOT NULL,
             collected_at_unix INTEGER NOT NULL,
             summary TEXT,
             analysis_status TEXT NOT NULL DEFAULT 'pending',
             ai_summary TEXT,
             sentiment TEXT,
             sentiment_score REAL,
             importance INTEGER,
             impact_horizon TEXT,
             related_stocks_json TEXT NOT NULL DEFAULT '[]',
             rationale TEXT,
             analysis_model TEXT,
             analyzed_at_unix INTEGER,
             analysis_error TEXT,
             analysis_attempts INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_news_published
             ON news_articles(published_at_unix DESC);",
    )?;
    ensure_analysis_columns(&connection)?;
    Ok(())
}

pub async fn start_collector(state: &AppState) {
    let mut runtime = state.news_collector.lock().await;
    if runtime.handle.is_some() {
        return;
    }

    let interval_hours = state.config.news_collection_interval_hours.clamp(1, 168);
    runtime.status.running = true;
    runtime.status.interval_hours = interval_hours;
    runtime.status.retention_days = state.config.news_retention_days.max(1);
    runtime.status.daily_limit = state.config.news_daily_limit.max(1);
    runtime.status.max_database_bytes = state.config.news_max_database_bytes.max(1_048_576);
    runtime.status.analysis_enabled = state.config.news_analysis_enabled;

    let worker_state = state.clone();
    runtime.handle = Some(tokio::spawn(async move {
        loop {
            if let Err((_, Json(error))) = collect_once(&worker_state).await {
                tracing::warn!("news collection failed: {}", error.message);
            }
            sleep(Duration::from_secs(interval_hours * 3600)).await;
        }
    }));
}

pub async fn collect_once(state: &AppState) -> ApiResult<NewsCollectorStatus> {
    {
        let mut runtime = state.news_collector.lock().await;
        if runtime.status.collecting {
            return Err(api_error(
                StatusCode::CONFLICT,
                "news_collection_in_progress",
                "News collection is already running.",
            ));
        }
        runtime.status.collecting = true;
        runtime.status.last_started_at_unix = Some(unix_now());
        runtime.status.last_error = None;
    }

    let result = fetch_and_store(state).await;
    let analysis_result = if result.is_ok() && state.config.news_analysis_enabled {
        Some(analyze_pending(state).await)
    } else {
        None
    };
    let mut runtime = state.news_collector.lock().await;
    runtime.status.collecting = false;
    runtime.status.last_finished_at_unix = Some(unix_now());
    runtime.status.next_collection_at_unix =
        Some(unix_now() + state.config.news_collection_interval_hours.clamp(1, 168) * 3600);

    match result {
        Ok(inserted) => {
            runtime.status.last_inserted_count = inserted;
            runtime.status.last_analysis_error = analysis_result
                .and_then(Result::err)
                .map(|(_, Json(error))| error.message);
            refresh_storage_stats(state, &mut runtime.status)?;
            Ok(runtime.status.clone())
        }
        Err(error) => {
            runtime.status.last_error = Some(error.to_string());
            Err(api_error(
                StatusCode::BAD_GATEWAY,
                "news_collection_failed",
                error,
            ))
        }
    }
}

pub async fn status(state: &AppState) -> ApiResult<NewsCollectorStatus> {
    let mut runtime = state.news_collector.lock().await;
    refresh_storage_stats(state, &mut runtime.status)?;
    Ok(runtime.status.clone())
}

pub fn list(state: &AppState, limit: usize) -> ApiResult<Vec<NewsArticle>> {
    let connection = open_database(state).map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "SELECT id, title, url, source, published_at_unix, collected_at_unix, summary,
                    analysis_status, ai_summary, sentiment, sentiment_score, importance,
                    impact_horizon, related_stocks_json, rationale, analysis_model,
                    analyzed_at_unix, analysis_error
             FROM news_articles ORDER BY published_at_unix DESC LIMIT ?1",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([limit.clamp(1, 100) as i64], |row| {
            Ok(NewsArticle {
                id: row.get(0)?,
                title: row.get(1)?,
                url: row.get(2)?,
                source: row.get(3)?,
                published_at_unix: row.get(4)?,
                collected_at_unix: row.get(5)?,
                summary: row.get(6)?,
                analysis_status: row.get(7)?,
                ai_summary: row.get(8)?,
                sentiment: row.get(9)?,
                sentiment_score: row.get(10)?,
                importance: row.get(11)?,
                impact_horizon: row.get(12)?,
                related_stocks: serde_json::from_str::<Vec<Value>>(&row.get::<_, String>(13)?)
                    .unwrap_or_default(),
                rationale: row.get(14)?,
                analysis_model: row.get(15)?,
                analyzed_at_unix: row.get(16)?,
                analysis_error: row.get(17)?,
            })
        })
        .map_err(database_error)?;

    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

pub async fn analyze_pending(state: &AppState) -> ApiResult<NewsAnalysisRun> {
    if !state.config.news_analysis_enabled {
        return Err(api_error(
            StatusCode::PRECONDITION_FAILED,
            "news_analysis_disabled",
            "NEWS_ANALYSIS_ENABLED must be true.",
        ));
    }

    let articles = pending_articles(state)?;
    if articles.is_empty() {
        return Ok(NewsAnalysisRun {
            requested: 0,
            analyzed: 0,
            failed: 0,
            model: None,
        });
    }

    let requested = articles.len();
    match strategy::analyze_news(state, articles.clone()).await {
        Ok(response) => {
            let connection = open_database(state).map_err(database_error)?;
            let now = unix_now() as i64;
            for analysis in &response.analyses {
                let related_stocks =
                    serde_json::to_string(&analysis.related_stocks).map_err(database_error)?;
                connection
                    .execute(
                        "UPDATE news_articles SET
                            analysis_status = 'analyzed', ai_summary = ?1, sentiment = ?2,
                            sentiment_score = ?3, importance = ?4, impact_horizon = ?5,
                            related_stocks_json = ?6, rationale = ?7, analysis_model = ?8,
                            analyzed_at_unix = ?9, analysis_error = NULL,
                            analysis_attempts = analysis_attempts + 1
                         WHERE id = ?10",
                        params![
                            analysis.summary,
                            analysis.sentiment,
                            analysis.sentiment_score,
                            analysis.importance,
                            analysis.impact_horizon,
                            related_stocks,
                            analysis.rationale,
                            response.model,
                            now,
                            analysis.article_id,
                        ],
                    )
                    .map_err(database_error)?;
            }
            Ok(NewsAnalysisRun {
                requested,
                analyzed: response.analyses.len(),
                failed: requested.saturating_sub(response.analyses.len()),
                model: Some(response.model),
            })
        }
        Err(error) => {
            mark_analysis_failed(state, &articles, &error.to_string())?;
            Err(api_error(
                StatusCode::BAD_GATEWAY,
                "news_analysis_failed",
                error,
            ))
        }
    }
}

async fn fetch_and_store(state: &AppState) -> anyhow::Result<u32> {
    let now = unix_now() as i64;
    let connection = open_database(state)?;
    connection.execute(
        "DELETE FROM news_articles WHERE collected_at_unix < ?1",
        [now - (state.config.news_retention_days.max(1) as i64 * 86_400)],
    )?;

    let collected_today: u32 = connection.query_row(
        "SELECT COUNT(*) FROM news_articles WHERE collected_at_unix >= ?1",
        [now - 86_400],
        |row| row.get(0),
    )?;
    let mut remaining = state
        .config
        .news_daily_limit
        .saturating_sub(collected_today);
    let mut inserted = 0;

    for feed_url in &state.config.news_feed_urls {
        if remaining == 0 {
            break;
        }
        let bytes = state
            .http
            .get(feed_url)
            .header("user-agent", "ai-stock-news-collector/0.1")
            .timeout(Duration::from_secs(20))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        let feed = parser::parse(bytes.as_ref())?;
        let feed_source = feed
            .title
            .as_ref()
            .map(|title| title.content.clone())
            .unwrap_or_else(|| "뉴스 피드".to_string());

        for entry in feed.entries {
            if remaining == 0 {
                break;
            }
            let Some(title) = entry.title.map(|title| clean_text(&title.content, 240)) else {
                continue;
            };
            let Some(url) = entry.links.first().map(|link| link.href.clone()) else {
                continue;
            };
            let published_at = entry
                .published
                .or(entry.updated)
                .map(|time| time.timestamp())
                .unwrap_or(now);
            let summary = entry
                .summary
                .map(|summary| clean_text(&summary.content, 500))
                .filter(|summary| !summary.is_empty());
            let source = title
                .rsplit_once(" - ")
                .map(|(_, source)| source.to_string())
                .unwrap_or_else(|| feed_source.clone());

            let changed = connection.execute(
                "INSERT OR IGNORE INTO news_articles
                 (title, url, source, published_at_unix, collected_at_unix, summary)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![title, url, source, published_at, now, summary],
            )?;
            if changed > 0 {
                inserted += 1;
                remaining -= 1;
            }
        }
    }
    enforce_storage_limit(
        &connection,
        &state.config.news_db_path,
        state.config.news_max_database_bytes.max(1_048_576),
    )?;
    Ok(inserted)
}

fn enforce_storage_limit(
    connection: &Connection,
    database_path: &str,
    max_bytes: u64,
) -> anyhow::Result<()> {
    connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    let size = fs::metadata(database_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    if size <= max_bytes {
        return Ok(());
    }

    let article_count: u64 =
        connection.query_row("SELECT COUNT(*) FROM news_articles", [], |row| row.get(0))?;
    let target_count = article_count
        .saturating_mul(max_bytes.saturating_mul(9) / 10)
        .checked_div(size.max(1))
        .unwrap_or(0);
    let delete_count = article_count.saturating_sub(target_count).max(1);
    connection.execute(
        "DELETE FROM news_articles WHERE id IN (
            SELECT id FROM news_articles ORDER BY published_at_unix ASC LIMIT ?1
        )",
        [delete_count],
    )?;
    connection.execute_batch("VACUUM;")?;
    Ok(())
}

fn refresh_storage_stats(state: &AppState, status: &mut NewsCollectorStatus) -> ApiResult<()> {
    let connection = open_database(state).map_err(database_error)?;
    status.article_count = connection
        .query_row("SELECT COUNT(*) FROM news_articles", [], |row| row.get(0))
        .map_err(database_error)?;
    status.database_bytes = ["", "-wal", "-shm"]
        .iter()
        .map(|suffix| format!("{}{}", state.config.news_db_path, suffix))
        .filter_map(|path| fs::metadata(path).ok())
        .map(|metadata| metadata.len())
        .sum();
    status.analyzed_count = count_by_analysis_status(&connection, "analyzed")?;
    status.pending_analysis_count = count_by_analysis_status(&connection, "pending")?;
    status.failed_analysis_count = count_by_analysis_status(&connection, "failed")?;
    Ok(())
}

fn count_by_analysis_status(connection: &Connection, value: &str) -> ApiResult<u64> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM news_articles WHERE analysis_status = ?1",
            [value],
            |row| row.get(0),
        )
        .map_err(database_error)
}

fn pending_articles(state: &AppState) -> ApiResult<Vec<NewsArticleInput>> {
    let connection = open_database(state).map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "SELECT id, title, source, summary FROM news_articles
             WHERE analysis_status IN ('pending', 'failed') AND analysis_attempts < 3
             ORDER BY published_at_unix DESC LIMIT ?1",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([state.config.news_analysis_batch_size as i64], |row| {
            Ok(NewsArticleInput {
                id: row.get(0)?,
                title: row.get(1)?,
                source: row.get(2)?,
                summary: row.get(3)?,
            })
        })
        .map_err(database_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn mark_analysis_failed(
    state: &AppState,
    articles: &[NewsArticleInput],
    error: &str,
) -> ApiResult<()> {
    let connection = open_database(state).map_err(database_error)?;
    let error = error.chars().take(500).collect::<String>();
    for article in articles {
        connection
            .execute(
                "UPDATE news_articles SET analysis_status = 'failed', analysis_error = ?1,
                 analysis_attempts = analysis_attempts + 1 WHERE id = ?2",
                params![error, article.id],
            )
            .map_err(database_error)?;
    }
    Ok(())
}

fn ensure_analysis_columns(connection: &Connection) -> anyhow::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(news_articles)")?;
    let existing = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    let columns = [
        ("analysis_status", "TEXT NOT NULL DEFAULT 'pending'"),
        ("ai_summary", "TEXT"),
        ("sentiment", "TEXT"),
        ("sentiment_score", "REAL"),
        ("importance", "INTEGER"),
        ("impact_horizon", "TEXT"),
        ("related_stocks_json", "TEXT NOT NULL DEFAULT '[]'"),
        ("rationale", "TEXT"),
        ("analysis_model", "TEXT"),
        ("analyzed_at_unix", "INTEGER"),
        ("analysis_error", "TEXT"),
        ("analysis_attempts", "INTEGER NOT NULL DEFAULT 0"),
    ];
    drop(statement);
    for (name, definition) in columns {
        if !existing.contains(name) {
            connection.execute_batch(&format!(
                "ALTER TABLE news_articles ADD COLUMN {name} {definition};"
            ))?;
        }
    }
    Ok(())
}

fn open_database(state: &AppState) -> anyhow::Result<Connection> {
    Ok(Connection::open(&state.config.news_db_path)?)
}

fn database_error(error: impl std::fmt::Display) -> (StatusCode, Json<crate::error::ApiError>) {
    api_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "news_database_failed",
        error,
    )
}

fn clean_text(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for character in value.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag && output.chars().count() < max_chars => output.push(character),
            _ => {}
        }
    }
    output
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::clean_text;

    #[test]
    fn removes_markup_and_limits_text() {
        assert_eq!(clean_text("<b>증시</b> 상승 소식", 6), "증시 상승");
        assert_eq!(clean_text("증시&nbsp;&amp; 환율", 30), "증시 & 환율");
    }
}
