use axum::{http::StatusCode, Json};
use feed_rs::parser;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::Path,
    time::Instant,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    task::JoinHandle,
    time::{sleep, Duration},
};

use crate::{
    error::{api_error, ApiResult},
    state::AppState,
    stocks,
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
    pub elapsed_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub estimated_cost_usd: Option<f64>,
}

#[derive(Clone, Serialize)]
pub struct StockNewsGroup {
    pub symbol: String,
    pub name: String,
    pub market: String,
    pub article_count: usize,
    pub event_count: usize,
    pub source_count: usize,
    pub latest_published_at_unix: i64,
    pub average_sentiment_score: Option<f64>,
    pub articles: Vec<NewsArticle>,
}

#[derive(Clone, Serialize)]
pub struct NewsEventGroup {
    pub event_key: String,
    pub headline: String,
    pub article_count: usize,
    pub source_count: usize,
    pub sources: Vec<String>,
    pub latest_published_at_unix: i64,
    pub average_sentiment_score: Option<f64>,
    pub max_importance: Option<u8>,
    pub evidence_confidence: f64,
    pub related_stocks: Vec<Value>,
    pub articles: Vec<NewsArticle>,
}

#[derive(Clone, Serialize)]
pub struct DailyStockOutlook {
    pub day_key: i64,
    pub symbol: String,
    pub name: String,
    pub action: String,
    pub confidence: f64,
    pub impact_horizon: String,
    pub reasons: Vec<String>,
    pub event_keys: Vec<String>,
    pub model: String,
    pub generated_at_unix: i64,
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
             ON news_articles(published_at_unix DESC);
         CREATE TABLE IF NOT EXISTS stock_daily_outlooks (
             day_key INTEGER NOT NULL,
             symbol TEXT NOT NULL,
             name TEXT NOT NULL,
             action TEXT NOT NULL,
             confidence REAL NOT NULL,
             impact_horizon TEXT NOT NULL,
             reasons_json TEXT NOT NULL,
             event_keys_json TEXT NOT NULL,
             model TEXT NOT NULL,
             generated_at_unix INTEGER NOT NULL,
             PRIMARY KEY (day_key, symbol)
         );",
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

pub fn grouped_by_stock(state: &AppState, limit: usize) -> ApiResult<Vec<StockNewsGroup>> {
    let events = grouped_events(state, limit)?;
    let mut groups = BTreeMap::<String, StockNewsGroup>::new();
    let mut sentiment_sums = BTreeMap::<String, (f64, usize)>::new();
    let mut source_names = BTreeMap::<String, BTreeSet<String>>::new();

    for event in events {
        for related_stock in &event.related_stocks {
            let Some(symbol) = related_stock.get("symbol").and_then(Value::as_str) else {
                continue;
            };
            if !is_korean_stock_symbol(symbol) {
                continue;
            }
            let Some(name) = related_stock.get("name").and_then(Value::as_str) else {
                continue;
            };
            let market = related_stock
                .get("market")
                .and_then(Value::as_str)
                .unwrap_or("KRX");
            let group = groups
                .entry(symbol.to_string())
                .or_insert_with(|| StockNewsGroup {
                    symbol: symbol.to_string(),
                    name: name.to_string(),
                    market: market.to_string(),
                    article_count: 0,
                    event_count: 0,
                    source_count: 0,
                    latest_published_at_unix: event.latest_published_at_unix,
                    average_sentiment_score: None,
                    articles: Vec::new(),
                });
            group.article_count += event.article_count;
            group.event_count += 1;
            group.latest_published_at_unix = group
                .latest_published_at_unix
                .max(event.latest_published_at_unix);
            for article in &event.articles {
                if group.articles.len() < 5 {
                    group.articles.push(article.clone());
                }
            }
            source_names
                .entry(symbol.to_string())
                .or_default()
                .extend(event.sources.iter().cloned());
            if let Some(score) = event.average_sentiment_score {
                let entry = sentiment_sums.entry(symbol.to_string()).or_insert((0.0, 0));
                entry.0 += score;
                entry.1 += 1;
            }
        }
    }

    for (symbol, (sum, count)) in sentiment_sums {
        if let Some(group) = groups.get_mut(&symbol) {
            group.average_sentiment_score = Some(sum / count as f64);
        }
    }
    for (symbol, sources) in source_names {
        if let Some(group) = groups.get_mut(&symbol) {
            group.source_count = sources.len();
        }
    }

    let mut groups = groups.into_values().collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right.event_count.cmp(&left.event_count).then_with(|| {
            right
                .latest_published_at_unix
                .cmp(&left.latest_published_at_unix)
        })
    });
    Ok(groups)
}

pub fn grouped_events(state: &AppState, limit: usize) -> ApiResult<Vec<NewsEventGroup>> {
    Ok(cluster_articles(list(state, limit)?))
}

pub fn daily_outlooks(state: &AppState) -> ApiResult<Vec<DailyStockOutlook>> {
    let connection = open_database(state).map_err(database_error)?;
    let day_key = korea_day_key(unix_now() as i64);
    let mut statement = connection
        .prepare(
            "SELECT day_key, symbol, name, action, confidence, impact_horizon,
                    reasons_json, event_keys_json, model, generated_at_unix
             FROM stock_daily_outlooks WHERE day_key = ?1
             ORDER BY confidence DESC, symbol ASC",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([day_key], |row| {
            Ok(DailyStockOutlook {
                day_key: row.get(0)?,
                symbol: row.get(1)?,
                name: row.get(2)?,
                action: row.get(3)?,
                confidence: row.get(4)?,
                impact_horizon: row.get(5)?,
                reasons: serde_json::from_str(&row.get::<_, String>(6)?).unwrap_or_default(),
                event_keys: serde_json::from_str(&row.get::<_, String>(7)?).unwrap_or_default(),
                model: row.get(8)?,
                generated_at_unix: row.get(9)?,
            })
        })
        .map_err(database_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

pub async fn generate_daily_outlooks(state: &AppState) -> ApiResult<Vec<DailyStockOutlook>> {
    if !state.config.news_analysis_enabled {
        return Err(api_error(
            StatusCode::PRECONDITION_FAILED,
            "news_analysis_disabled",
            "NEWS_ANALYSIS_ENABLED must be true.",
        ));
    }
    let cutoff = unix_now() as i64 - 24 * 3600;
    let mut stocks_by_symbol = BTreeMap::<String, strategy::StockOutlookInput>::new();
    for event in grouped_events(state, 100)?
        .into_iter()
        .filter(|event| event.latest_published_at_unix >= cutoff)
    {
        for stock in &event.related_stocks {
            let Some(symbol) = stock.get("symbol").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = stock.get("name").and_then(Value::as_str) else {
                continue;
            };
            let input = stocks_by_symbol
                .entry(symbol.to_string())
                .or_insert_with(|| strategy::StockOutlookInput {
                    symbol: symbol.to_string(),
                    name: name.to_string(),
                    events: Vec::new(),
                });
            if input.events.len() < 20 {
                input.events.push(strategy::StockEventInput {
                    event_key: event.event_key.clone(),
                    headline: event.headline.clone(),
                    sentiment_score: event.average_sentiment_score,
                    importance: event.max_importance,
                    impact_horizon: event
                        .articles
                        .iter()
                        .filter_map(|article| article.impact_horizon.clone())
                        .next(),
                    source_count: event.source_count,
                    evidence_confidence: event.evidence_confidence,
                });
            }
        }
    }
    let inputs = stocks_by_symbol.into_values().take(20).collect::<Vec<_>>();
    if inputs.is_empty() {
        return Err(api_error(
            StatusCode::PRECONDITION_FAILED,
            "no_daily_news_events",
            "No confirmed stock news events were published in the last 24 hours.",
        ));
    }
    let event_keys_by_symbol = inputs
        .iter()
        .map(|stock| {
            (
                stock.symbol.clone(),
                (
                    stock.name.clone(),
                    stock
                        .events
                        .iter()
                        .map(|event| event.event_key.clone())
                        .collect::<Vec<_>>(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let response = strategy::generate_daily_outlooks(state, inputs)
        .await
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, "daily_outlook_failed", error))?;
    let now = unix_now() as i64;
    let day_key = korea_day_key(now);
    let mut connection = open_database(state).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    for outlook in response.outlooks {
        let Some((name, event_keys)) = event_keys_by_symbol.get(&outlook.symbol) else {
            continue;
        };
        transaction
            .execute(
                "INSERT INTO stock_daily_outlooks
                    (day_key, symbol, name, action, confidence, impact_horizon,
                     reasons_json, event_keys_json, model, generated_at_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(day_key, symbol) DO UPDATE SET
                    name = excluded.name, action = excluded.action,
                    confidence = excluded.confidence, impact_horizon = excluded.impact_horizon,
                    reasons_json = excluded.reasons_json,
                    event_keys_json = excluded.event_keys_json, model = excluded.model,
                    generated_at_unix = excluded.generated_at_unix",
                params![
                    day_key,
                    outlook.symbol,
                    name,
                    outlook.action,
                    outlook.confidence,
                    outlook.impact_horizon,
                    serde_json::to_string(&outlook.reasons).map_err(database_error)?,
                    serde_json::to_string(event_keys).map_err(database_error)?,
                    response.model,
                    now,
                ],
            )
            .map_err(database_error)?;
    }
    transaction.commit().map_err(database_error)?;
    daily_outlooks(state)
}

fn korea_day_key(timestamp: i64) -> i64 {
    (timestamp + 9 * 3600).div_euclid(24 * 3600)
}

fn cluster_articles(articles: Vec<NewsArticle>) -> Vec<NewsEventGroup> {
    struct EventCandidate {
        tokens: HashSet<String>,
        stock_symbols: HashSet<String>,
        articles: Vec<NewsArticle>,
    }

    let mut candidates = Vec::<EventCandidate>::new();
    for article in articles {
        let tokens = event_tokens(&article.title);
        let stock_symbols = article_stock_symbols(&article);
        let matching_index = candidates.iter().position(|candidate| {
            let latest = candidate
                .articles
                .first()
                .map(|item| item.published_at_unix)
                .unwrap_or(article.published_at_unix);
            latest.abs_diff(article.published_at_unix) <= 72 * 3600
                && is_same_event(
                    &tokens,
                    &stock_symbols,
                    &candidate.tokens,
                    &candidate.stock_symbols,
                )
        });

        if let Some(index) = matching_index {
            candidates[index].tokens.extend(tokens);
            candidates[index].stock_symbols.extend(stock_symbols);
            candidates[index].articles.push(article);
        } else {
            candidates.push(EventCandidate {
                tokens,
                stock_symbols,
                articles: vec![article],
            });
        }
    }

    candidates
        .into_iter()
        .map(|candidate| build_event_group(candidate.articles))
        .collect()
}

fn build_event_group(articles: Vec<NewsArticle>) -> NewsEventGroup {
    let headline = articles
        .first()
        .map(|article| article.title.clone())
        .unwrap_or_default();
    let sources = articles
        .iter()
        .map(|article| article.source.trim().to_string())
        .filter(|source| !source.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let sentiment_scores = articles
        .iter()
        .filter_map(|article| article.sentiment_score)
        .collect::<Vec<_>>();
    let analyzed_count = articles
        .iter()
        .filter(|article| article.analysis_status == "analyzed")
        .count();
    let max_importance = articles
        .iter()
        .filter_map(|article| article.importance)
        .max();
    let mut related_by_symbol = BTreeMap::<String, Value>::new();
    for article in &articles {
        for stock in &article.related_stocks {
            if let Some(symbol) = stock.get("symbol").and_then(Value::as_str) {
                if is_korean_stock_symbol(symbol) {
                    related_by_symbol
                        .entry(symbol.to_string())
                        .or_insert_with(|| stock.clone());
                }
            }
        }
    }
    let source_count = sources.len();
    let evidence_confidence =
        event_evidence_confidence(source_count, articles.len(), analyzed_count, max_importance);
    let latest_published_at_unix = articles
        .iter()
        .map(|article| article.published_at_unix)
        .max()
        .unwrap_or_default();
    let event_identity = format!(
        "{}|{}",
        normalize_event_title(&headline),
        latest_published_at_unix.div_euclid(72 * 3600)
    );

    NewsEventGroup {
        event_key: format!("evt-{:016x}", fnv1a64(&event_identity)),
        headline,
        article_count: articles.len(),
        source_count,
        sources,
        latest_published_at_unix,
        average_sentiment_score: (!sentiment_scores.is_empty())
            .then(|| sentiment_scores.iter().sum::<f64>() / sentiment_scores.len() as f64),
        max_importance,
        evidence_confidence,
        related_stocks: related_by_symbol.into_values().collect(),
        articles,
    }
}

fn is_same_event(
    left_tokens: &HashSet<String>,
    left_stocks: &HashSet<String>,
    right_tokens: &HashSet<String>,
    right_stocks: &HashSet<String>,
) -> bool {
    let common = left_tokens.intersection(right_tokens).count();
    if common < 2 {
        return false;
    }
    let union = left_tokens.union(right_tokens).count();
    let similarity = common as f64 / union.max(1) as f64;
    let shares_stock = left_stocks
        .iter()
        .any(|symbol| right_stocks.contains(symbol));
    similarity >= 0.6 || (shares_stock && similarity >= 0.4)
}

fn article_stock_symbols(article: &NewsArticle) -> HashSet<String> {
    article
        .related_stocks
        .iter()
        .filter_map(|stock| stock.get("symbol").and_then(Value::as_str))
        .filter(|symbol| is_korean_stock_symbol(symbol))
        .map(str::to_string)
        .collect()
}

fn event_tokens(title: &str) -> HashSet<String> {
    normalize_event_title(title)
        .split_whitespace()
        .filter(|token| token.chars().count() >= 2)
        .filter(|token| !EVENT_STOP_WORDS.contains(token))
        .map(str::to_string)
        .collect()
}

fn normalize_event_title(title: &str) -> String {
    let without_source = title
        .rsplit_once(" - ")
        .map(|(title, _)| title)
        .unwrap_or(title);
    let mut normalized = String::with_capacity(without_source.len());
    let mut bracket_depth = 0_u8;
    for character in without_source.chars().flat_map(char::to_lowercase) {
        match character {
            '[' | '(' | '<' | '【' => bracket_depth = bracket_depth.saturating_add(1),
            ']' | ')' | '>' | '】' => bracket_depth = bracket_depth.saturating_sub(1),
            _ if bracket_depth > 0 => {}
            _ if character.is_alphanumeric() || ('가'..='힣').contains(&character) => {
                normalized.push(character)
            }
            _ => normalized.push(' '),
        }
    }
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn event_evidence_confidence(
    source_count: usize,
    article_count: usize,
    analyzed_count: usize,
    max_importance: Option<u8>,
) -> f64 {
    let source_score = match source_count {
        0 => 0.2,
        1 => 0.45,
        2 => 0.65,
        _ => 0.8,
    };
    let analysis_score = if article_count == 0 {
        0.0
    } else {
        analyzed_count as f64 / article_count as f64 * 0.1
    };
    let importance_score = if max_importance.unwrap_or_default() >= 4 {
        0.1
    } else {
        0.0
    };
    (source_score + analysis_score + importance_score).min(1.0)
}

fn fnv1a64(value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(0xcbf29ce484222325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        })
}

const EVENT_STOP_WORDS: &[&str] = &[
    "관련",
    "대한",
    "통해",
    "위한",
    "오늘",
    "단독",
    "속보",
    "종합",
    "뉴스",
    "코스피",
    "코스닥",
];

fn is_korean_stock_symbol(value: &str) -> bool {
    value.len() == 6 && value.chars().all(|char| char.is_ascii_digit())
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
            elapsed_ms: 0,
            input_tokens: None,
            output_tokens: None,
            total_tokens: None,
            estimated_cost_usd: None,
        });
    }

    let requested = articles.len();
    let started_at = Instant::now();
    match strategy::analyze_news(state, articles.clone()).await {
        Ok(response) => {
            let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            let connection = open_database(state).map_err(database_error)?;
            let now = unix_now() as i64;
            for analysis in &response.analyses {
                let related_stocks = resolve_related_stocks(state, &analysis.related_stocks)?;
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
            let input_tokens = response.usage.as_ref().and_then(|usage| usage.input_tokens);
            let output_tokens = response
                .usage
                .as_ref()
                .and_then(|usage| usage.output_tokens);
            let total_tokens = response.usage.as_ref().and_then(|usage| usage.total_tokens);
            Ok(NewsAnalysisRun {
                requested,
                analyzed: response.analyses.len(),
                failed: requested.saturating_sub(response.analyses.len()),
                estimated_cost_usd: estimate_news_analysis_cost(
                    &response.model,
                    input_tokens,
                    output_tokens,
                ),
                model: Some(response.model),
                elapsed_ms,
                input_tokens,
                output_tokens,
                total_tokens,
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

fn resolve_related_stocks(
    state: &AppState,
    related_stocks: &[strategy::RelatedStock],
) -> ApiResult<String> {
    let mut resolved = Vec::new();
    for related in related_stocks {
        let Some(stock) = stocks::resolve_news_stock(state, &related.name, &related.symbol)? else {
            continue;
        };
        resolved.push(serde_json::json!({
            "name": stock.name,
            "symbol": stock.symbol,
            "market": stock.market,
            "relevance": related.relevance,
            "ai_name": related.name,
            "ai_symbol": related.symbol,
        }));
    }
    serde_json::to_string(&resolved).map_err(database_error)
}

fn estimate_news_analysis_cost(
    model: &str,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
) -> Option<f64> {
    let (input_per_million, output_per_million) = match model {
        // OpenAI API pricing for gpt-4o-mini: $0.15 / 1M input, $0.60 / 1M output.
        "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => (0.15, 0.60),
        _ => return None,
    };
    Some(
        (input_tokens? as f64 * input_per_million + output_tokens? as f64 * output_per_million)
            / 1_000_000.0,
    )
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
    use super::*;

    fn article(
        id: i64,
        title: &str,
        source: &str,
        published_at_unix: i64,
        symbol: Option<&str>,
    ) -> NewsArticle {
        NewsArticle {
            id,
            title: title.to_string(),
            url: format!("https://example.com/{id}"),
            source: source.to_string(),
            published_at_unix,
            collected_at_unix: published_at_unix,
            summary: None,
            analysis_status: "analyzed".to_string(),
            ai_summary: None,
            sentiment: Some("positive".to_string()),
            sentiment_score: Some(0.6),
            importance: Some(4),
            impact_horizon: Some("short_term".to_string()),
            related_stocks: symbol
                .map(|symbol| {
                    vec![serde_json::json!({
                        "symbol": symbol,
                        "name": "삼성전자",
                        "market": "KOSPI"
                    })]
                })
                .unwrap_or_default(),
            rationale: None,
            analysis_model: None,
            analyzed_at_unix: Some(published_at_unix),
            analysis_error: None,
        }
    }

    #[test]
    fn removes_markup_and_limits_text() {
        assert_eq!(clean_text("<b>증시</b> 상승 소식", 6), "증시 상승");
        assert_eq!(clean_text("증시&nbsp;&amp; 환율", 30), "증시 & 환율");
    }

    #[test]
    fn groups_similar_articles_from_multiple_sources() {
        let events = cluster_articles(vec![
            article(
                1,
                "삼성전자 HBM 공급 확대 기대감에 주가 상승 - 연합뉴스",
                "연합뉴스",
                1_000_000,
                Some("005930"),
            ),
            article(
                2,
                "삼성전자 HBM 공급 확대 기대감 주가 상승 - 한국경제",
                "한국경제",
                999_000,
                Some("005930"),
            ),
        ]);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].article_count, 2);
        assert_eq!(events[0].source_count, 2);
        assert!(events[0].evidence_confidence > 0.7);
    }

    #[test]
    fn keeps_distinct_events_for_same_stock_separate() {
        let events = cluster_articles(vec![
            article(
                1,
                "삼성전자 HBM 공급 확대 기대감에 주가 상승",
                "연합뉴스",
                1_000_000,
                Some("005930"),
            ),
            article(
                2,
                "삼성전자 노조 임금 협상 결렬 파업 예고",
                "한국경제",
                999_000,
                Some("005930"),
            ),
        ]);

        assert_eq!(events.len(), 2);
    }

    #[test]
    fn does_not_group_articles_outside_event_window() {
        let events = cluster_articles(vec![
            article(
                1,
                "삼성전자 HBM 공급 확대 기대감에 주가 상승",
                "연합뉴스",
                1_000_000,
                Some("005930"),
            ),
            article(
                2,
                "삼성전자 HBM 공급 확대 기대감 주가 상승",
                "한국경제",
                1_000_000 - 73 * 3600,
                Some("005930"),
            ),
        ]);

        assert_eq!(events.len(), 2);
        assert_ne!(events[0].event_key, events[1].event_key);
    }
}
