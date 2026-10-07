use axum::Json;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::time::{sleep, Duration};

use crate::{
    error::{api_error, ApiResult},
    news::NewsTradeDecision,
    state::AppState,
};

const EVALUATION_HORIZONS: [i64; 3] = [1, 3, 5];

#[derive(Clone, Serialize)]
pub struct NewsPerformancePoint {
    pub decision_day_key: i64,
    pub symbol: String,
    pub name: String,
    pub side: String,
    pub confidence: f64,
    pub sentiment_score: Option<f64>,
    pub horizon_days: i64,
    pub baseline_price: u64,
    pub target_at_unix: i64,
    pub evaluated_price: Option<u64>,
    pub raw_return_pct: Option<f64>,
    pub directional_return_pct: Option<f64>,
    pub hit: Option<bool>,
    pub status: String,
    pub evaluated_at_unix: Option<i64>,
    pub event_keys: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct NewsPerformanceSummary {
    pub tracked_signals: u64,
    pub evaluated_points: u64,
    pub pending_points: u64,
    pub directional_samples: u64,
    pub hit_rate_pct: Option<f64>,
    pub average_directional_return_pct: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub by_stock: Vec<StockPerformanceSummary>,
    pub by_sentiment: Vec<SentimentPerformanceSummary>,
}

#[derive(Clone, Serialize)]
pub struct StockPerformanceSummary {
    pub symbol: String,
    pub name: String,
    pub samples: u64,
    pub hit_rate_pct: f64,
    pub average_directional_return_pct: f64,
}

#[derive(Clone, Serialize)]
pub struct SentimentPerformanceSummary {
    pub sentiment: String,
    pub samples: u64,
    pub hit_rate_pct: f64,
    pub average_directional_return_pct: f64,
}

#[derive(Clone, Serialize)]
pub struct NewsPerformanceRun {
    pub due_points: usize,
    pub evaluated_points: usize,
    pub failed_symbols: usize,
    pub evaluated_at_unix: i64,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct NewsPerformanceSettings {
    pub enabled: bool,
    pub minimum_samples: u64,
    pub minimum_hit_rate_pct: f64,
    pub minimum_average_directional_return_pct: f64,
    pub maximum_drawdown_pct: f64,
}

impl Default for NewsPerformanceSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            minimum_samples: 20,
            minimum_hit_rate_pct: 45.0,
            minimum_average_directional_return_pct: 0.0,
            maximum_drawdown_pct: 10.0,
        }
    }
}

#[derive(Clone)]
pub struct NewsPerformancePolicy {
    settings: NewsPerformanceSettings,
    stock_metrics: HashMap<String, SignalMetrics>,
    sentiment_metrics: HashMap<String, SignalMetrics>,
    event_sentiments: HashMap<String, f64>,
}

#[derive(Clone)]
struct SignalMetrics {
    samples: usize,
    hit_rate_pct: f64,
    average_directional_return_pct: f64,
    maximum_drawdown_pct: f64,
}

#[derive(Clone)]
struct PendingPoint {
    decision_day_key: i64,
    symbol: String,
    side: String,
    horizon_days: i64,
    baseline_price: u64,
}

pub fn initialize(state: &AppState) -> anyhow::Result<()> {
    if let Some(parent) = Path::new(&state.config.news_db_path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let connection = open_database(state)?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS news_trade_performance (
            decision_day_key INTEGER NOT NULL,
            symbol TEXT NOT NULL,
            name TEXT NOT NULL,
            side TEXT NOT NULL,
            confidence REAL NOT NULL,
            sentiment_score REAL,
            horizon_days INTEGER NOT NULL,
            baseline_price INTEGER NOT NULL,
            target_at_unix INTEGER NOT NULL,
            evaluated_price INTEGER,
            raw_return_pct REAL,
            directional_return_pct REAL,
            hit INTEGER,
            status TEXT NOT NULL DEFAULT 'pending',
            evaluated_at_unix INTEGER,
            event_keys_json TEXT NOT NULL,
            PRIMARY KEY (decision_day_key, symbol, horizon_days)
         );
         CREATE INDEX IF NOT EXISTS idx_news_performance_due
            ON news_trade_performance(status, target_at_unix);
         CREATE TABLE IF NOT EXISTS news_performance_settings (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            enabled INTEGER NOT NULL,
            minimum_samples INTEGER NOT NULL,
            minimum_hit_rate_pct REAL NOT NULL,
            minimum_average_directional_return_pct REAL NOT NULL,
            maximum_drawdown_pct REAL NOT NULL,
            updated_at_unix INTEGER NOT NULL
         );",
    )?;
    ensure_column(&connection, "sentiment_score", "REAL")?;
    let defaults = NewsPerformanceSettings::default();
    connection.execute(
        "INSERT OR IGNORE INTO news_performance_settings
            (id, enabled, minimum_samples, minimum_hit_rate_pct,
             minimum_average_directional_return_pct, maximum_drawdown_pct, updated_at_unix)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            i64::from(defaults.enabled),
            defaults.minimum_samples,
            defaults.minimum_hit_rate_pct,
            defaults.minimum_average_directional_return_pct,
            defaults.maximum_drawdown_pct,
            unix_now(),
        ],
    )?;
    Ok(())
}

pub fn start_evaluator(state: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err((_, Json(error))) = evaluate_due(&state).await {
                tracing::warn!("news performance evaluation failed: {}", error.message);
            }
            sleep(Duration::from_secs(3600)).await;
        }
    });
}

pub fn settings(state: &AppState) -> ApiResult<NewsPerformanceSettings> {
    let connection = open_database(state).map_err(database_error)?;
    connection
        .query_row(
            "SELECT enabled, minimum_samples, minimum_hit_rate_pct,
                    minimum_average_directional_return_pct, maximum_drawdown_pct
             FROM news_performance_settings WHERE id = 1",
            [],
            |row| {
                Ok(NewsPerformanceSettings {
                    enabled: row.get::<_, i64>(0)? != 0,
                    minimum_samples: row.get(1)?,
                    minimum_hit_rate_pct: row.get(2)?,
                    minimum_average_directional_return_pct: row.get(3)?,
                    maximum_drawdown_pct: row.get(4)?,
                })
            },
        )
        .optional()
        .map(|settings| settings.unwrap_or_default())
        .map_err(database_error)
}

pub fn save_settings(
    state: &AppState,
    settings: NewsPerformanceSettings,
) -> ApiResult<NewsPerformanceSettings> {
    validate_settings(&settings)?;
    let connection = open_database(state).map_err(database_error)?;
    connection
        .execute(
            "INSERT INTO news_performance_settings
                (id, enabled, minimum_samples, minimum_hit_rate_pct,
                 minimum_average_directional_return_pct, maximum_drawdown_pct, updated_at_unix)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                enabled = excluded.enabled,
                minimum_samples = excluded.minimum_samples,
                minimum_hit_rate_pct = excluded.minimum_hit_rate_pct,
                minimum_average_directional_return_pct = excluded.minimum_average_directional_return_pct,
                maximum_drawdown_pct = excluded.maximum_drawdown_pct,
                updated_at_unix = excluded.updated_at_unix",
            params![
                i64::from(settings.enabled),
                settings.minimum_samples,
                settings.minimum_hit_rate_pct,
                settings.minimum_average_directional_return_pct,
                settings.maximum_drawdown_pct,
                unix_now(),
            ],
        )
        .map_err(database_error)?;
    Ok(settings)
}

pub fn exclusion_policy(state: &AppState) -> ApiResult<NewsPerformancePolicy> {
    let settings = settings(state)?;
    if !settings.enabled {
        return Ok(NewsPerformancePolicy {
            settings,
            stock_metrics: HashMap::new(),
            sentiment_metrics: HashMap::new(),
            event_sentiments: HashMap::new(),
        });
    }

    let points = load_points(state, i64::MAX)?;
    let event_sentiments = crate::news::grouped_events(state, 100)?
        .into_iter()
        .filter_map(|event| {
            event
                .average_sentiment_score
                .map(|score| (event.event_key, score))
        })
        .collect();
    Ok(NewsPerformancePolicy {
        settings,
        stock_metrics: grouped_metrics(&points, |point| Some(point.symbol.clone())),
        sentiment_metrics: grouped_metrics(&points, |point| {
            point.sentiment_score.map(sentiment_key)
        }),
        event_sentiments,
    })
}

impl NewsPerformancePolicy {
    pub fn exclusion_reason(
        &self,
        symbol: &str,
        name: &str,
        event_keys: &[String],
    ) -> Option<String> {
        if !self.settings.enabled {
            return None;
        }
        if let Some(metrics) = self.stock_metrics.get(symbol) {
            if let Some(reason) = metrics_exclusion_reason(&self.settings, name, metrics) {
                return Some(reason);
            }
        }

        let scores = event_keys
            .iter()
            .filter_map(|event_key| self.event_sentiments.get(event_key).copied())
            .collect::<Vec<_>>();
        let average_sentiment =
            (!scores.is_empty()).then(|| scores.iter().sum::<f64>() / scores.len() as f64)?;
        let sentiment = sentiment_key(average_sentiment);
        self.sentiment_metrics.get(&sentiment).and_then(|metrics| {
            metrics_exclusion_reason(
                &self.settings,
                &format!("{} 감성", sentiment_label(&sentiment)),
                metrics,
            )
        })
    }
}

pub fn seed_decisions(state: &AppState, decisions: &[NewsTradeDecision]) -> ApiResult<()> {
    let event_sentiments = crate::news::grouped_events(state, 100)?
        .into_iter()
        .filter_map(|event| {
            event
                .average_sentiment_score
                .map(|score| (event.event_key, score))
        })
        .collect::<HashMap<_, _>>();
    let mut connection = open_database(state).map_err(database_error)?;
    let transaction = connection.transaction().map_err(database_error)?;
    for decision in decisions.iter().filter(|decision| decision.price > 0) {
        let sentiment_scores = decision
            .event_keys
            .iter()
            .filter_map(|event_key| event_sentiments.get(event_key).copied())
            .collect::<Vec<_>>();
        let sentiment_score = (!sentiment_scores.is_empty())
            .then(|| sentiment_scores.iter().sum::<f64>() / sentiment_scores.len() as f64);
        let event_keys_json =
            serde_json::to_string(&decision.event_keys).map_err(database_error)?;
        for horizon_days in EVALUATION_HORIZONS {
            transaction
                .execute(
                    "INSERT INTO news_trade_performance
                        (decision_day_key, symbol, name, side, confidence, sentiment_score,
                         horizon_days, baseline_price, target_at_unix, event_keys_json)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(decision_day_key, symbol, horizon_days) DO NOTHING",
                    params![
                        decision.day_key,
                        decision.symbol,
                        decision.name,
                        decision.side,
                        decision.confidence,
                        sentiment_score,
                        horizon_days,
                        decision.price,
                        decision.generated_at_unix + horizon_days * 86_400,
                        event_keys_json,
                    ],
                )
                .map_err(database_error)?;
        }
    }
    transaction.commit().map_err(database_error)?;
    Ok(())
}

pub fn list(state: &AppState) -> ApiResult<Vec<NewsPerformancePoint>> {
    load_points(state, 300)
}

fn load_points(state: &AppState, limit: i64) -> ApiResult<Vec<NewsPerformancePoint>> {
    let connection = open_database(state).map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "SELECT decision_day_key, symbol, name, side, confidence, sentiment_score,
                    horizon_days, baseline_price, target_at_unix, evaluated_price,
                    raw_return_pct, directional_return_pct, hit, status,
                    evaluated_at_unix, event_keys_json
             FROM news_trade_performance
             ORDER BY decision_day_key DESC, symbol ASC, horizon_days ASC
             LIMIT ?1",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([limit], |row| {
            let event_keys_json: String = row.get(15)?;
            Ok(NewsPerformancePoint {
                decision_day_key: row.get(0)?,
                symbol: row.get(1)?,
                name: row.get(2)?,
                side: row.get(3)?,
                confidence: row.get(4)?,
                sentiment_score: row.get(5)?,
                horizon_days: row.get(6)?,
                baseline_price: row.get(7)?,
                target_at_unix: row.get(8)?,
                evaluated_price: row.get(9)?,
                raw_return_pct: row.get(10)?,
                directional_return_pct: row.get(11)?,
                hit: row.get::<_, Option<i64>>(12)?.map(|value| value != 0),
                status: row.get(13)?,
                evaluated_at_unix: row.get(14)?,
                event_keys: serde_json::from_str(&event_keys_json).unwrap_or_default(),
            })
        })
        .map_err(database_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

pub fn summary(state: &AppState) -> ApiResult<NewsPerformanceSummary> {
    let points = load_points(state, i64::MAX)?;
    let tracked_signals = points
        .iter()
        .map(|point| (point.decision_day_key, point.symbol.as_str()))
        .collect::<HashSet<_>>()
        .len() as u64;
    let evaluated_points = points
        .iter()
        .filter(|point| point.status == "evaluated")
        .count() as u64;
    let pending_points = points
        .iter()
        .filter(|point| point.status == "pending")
        .count() as u64;
    let mut directional = points
        .iter()
        .filter(|point| point.horizon_days == 1)
        .filter_map(|point| {
            point
                .directional_return_pct
                .zip(point.evaluated_at_unix)
                .map(|(rate, evaluated_at)| (evaluated_at, rate, point.hit.unwrap_or(false)))
        })
        .collect::<Vec<_>>();
    directional.sort_by_key(|item| item.0);
    let directional_samples = directional.len() as u64;
    let hit_rate_pct = percentage(
        directional.iter().filter(|item| item.2).count() as f64,
        directional.len(),
    );
    let average_directional_return_pct = if directional.is_empty() {
        None
    } else {
        Some(directional.iter().map(|item| item.1).sum::<f64>() / directional.len() as f64)
    };
    let by_stock = stock_summaries(&points);
    let by_sentiment = sentiment_summaries(&points);

    Ok(NewsPerformanceSummary {
        tracked_signals,
        evaluated_points,
        pending_points,
        directional_samples,
        hit_rate_pct,
        average_directional_return_pct,
        max_drawdown_pct: maximum_drawdown(directional.iter().map(|item| item.1)),
        by_stock,
        by_sentiment,
    })
}

pub async fn evaluate_due(state: &AppState) -> ApiResult<NewsPerformanceRun> {
    let now = unix_now();
    let pending = pending_points(state, now)?;
    let due_points = pending.len();
    let mut by_symbol = BTreeMap::<String, Vec<PendingPoint>>::new();
    for point in pending {
        by_symbol
            .entry(point.symbol.clone())
            .or_default()
            .push(point);
    }

    let mut evaluated_points = 0;
    let mut failed_symbols = 0;
    for (index, (symbol, points)) in by_symbol.into_iter().enumerate() {
        if index > 0 {
            sleep(Duration::from_millis(1200)).await;
        }
        let quote = match crate::kis::get_price(state, &symbol).await {
            Ok(quote) => quote,
            Err(_) => {
                failed_symbols += 1;
                continue;
            }
        };
        let Some(price) = quote
            .output
            .as_ref()
            .and_then(|output| value_u64(output, "stck_prpr"))
        else {
            failed_symbols += 1;
            continue;
        };
        let connection = open_database(state).map_err(database_error)?;
        for point in points {
            let raw_return = calculate_return(point.baseline_price, price);
            let directional_return = directional_return(&point.side, raw_return);
            let hit = directional_return.map(|value| value > 0.0);
            evaluated_points += connection
                .execute(
                    "UPDATE news_trade_performance
                     SET evaluated_price = ?1, raw_return_pct = ?2,
                         directional_return_pct = ?3, hit = ?4,
                         status = 'evaluated', evaluated_at_unix = ?5
                     WHERE decision_day_key = ?6 AND symbol = ?7 AND horizon_days = ?8
                       AND status = 'pending'",
                    params![
                        price,
                        raw_return,
                        directional_return,
                        hit.map(i64::from),
                        now,
                        point.decision_day_key,
                        point.symbol,
                        point.horizon_days,
                    ],
                )
                .map_err(database_error)?;
        }
    }

    Ok(NewsPerformanceRun {
        due_points,
        evaluated_points,
        failed_symbols,
        evaluated_at_unix: now,
    })
}

fn pending_points(state: &AppState, now: i64) -> ApiResult<Vec<PendingPoint>> {
    let connection = open_database(state).map_err(database_error)?;
    let mut statement = connection
        .prepare(
            "SELECT decision_day_key, symbol, side, horizon_days, baseline_price
             FROM news_trade_performance
             WHERE status = 'pending' AND target_at_unix <= ?1
             ORDER BY target_at_unix ASC",
        )
        .map_err(database_error)?;
    let rows = statement
        .query_map([now], |row| {
            Ok(PendingPoint {
                decision_day_key: row.get(0)?,
                symbol: row.get(1)?,
                side: row.get(2)?,
                horizon_days: row.get(3)?,
                baseline_price: row.get(4)?,
            })
        })
        .map_err(database_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(database_error)
}

fn calculate_return(baseline_price: u64, evaluated_price: u64) -> f64 {
    if baseline_price == 0 {
        return 0.0;
    }
    (evaluated_price as f64 - baseline_price as f64) / baseline_price as f64 * 100.0
}

fn directional_return(side: &str, raw_return: f64) -> Option<f64> {
    match side {
        "buy" => Some(raw_return),
        "sell" => Some(-raw_return),
        _ => None,
    }
}

fn percentage(numerator: f64, denominator: usize) -> Option<f64> {
    (denominator > 0).then(|| numerator / denominator as f64 * 100.0)
}

fn grouped_metrics<F>(points: &[NewsPerformancePoint], key: F) -> HashMap<String, SignalMetrics>
where
    F: Fn(&NewsPerformancePoint) -> Option<String>,
{
    let mut grouped = HashMap::<String, Vec<(i64, f64, bool)>>::new();
    for point in points.iter().filter(|point| point.horizon_days == 1) {
        let Some(group_key) = key(point) else {
            continue;
        };
        let Some((rate, evaluated_at)) = point.directional_return_pct.zip(point.evaluated_at_unix)
        else {
            continue;
        };
        grouped.entry(group_key).or_default().push((
            evaluated_at,
            rate,
            point.hit.unwrap_or(false),
        ));
    }
    grouped
        .into_iter()
        .map(|(key, mut samples)| {
            samples.sort_by_key(|sample| sample.0);
            let sample_count = samples.len();
            let metrics = SignalMetrics {
                samples: sample_count,
                hit_rate_pct: samples.iter().filter(|sample| sample.2).count() as f64
                    / sample_count as f64
                    * 100.0,
                average_directional_return_pct: samples.iter().map(|sample| sample.1).sum::<f64>()
                    / sample_count as f64,
                maximum_drawdown_pct: maximum_drawdown(samples.iter().map(|sample| sample.1))
                    .unwrap_or_default(),
            };
            (key, metrics)
        })
        .collect()
}

fn metrics_exclusion_reason(
    settings: &NewsPerformanceSettings,
    label: &str,
    metrics: &SignalMetrics,
) -> Option<String> {
    if metrics.samples < settings.minimum_samples as usize {
        return None;
    }
    if metrics.hit_rate_pct < settings.minimum_hit_rate_pct {
        return Some(format!(
            "성과 자동 제외: {label} 1일 표본 {}건의 적중률 {:.1}%가 기준 {:.1}% 미만입니다.",
            metrics.samples, metrics.hit_rate_pct, settings.minimum_hit_rate_pct
        ));
    }
    if metrics.average_directional_return_pct < settings.minimum_average_directional_return_pct {
        return Some(format!(
            "성과 자동 제외: {label} 1일 평균 방향 수익률 {:.2}%가 기준 {:.2}% 미만입니다.",
            metrics.average_directional_return_pct, settings.minimum_average_directional_return_pct
        ));
    }
    if metrics.maximum_drawdown_pct > settings.maximum_drawdown_pct {
        return Some(format!(
            "성과 자동 제외: {label} 1일 누적 최대 낙폭 {:.2}%가 기준 {:.2}%를 초과했습니다.",
            metrics.maximum_drawdown_pct, settings.maximum_drawdown_pct
        ));
    }
    None
}

fn sentiment_key(score: f64) -> String {
    if score > 0.15 {
        "positive"
    } else if score < -0.15 {
        "negative"
    } else {
        "neutral"
    }
    .to_string()
}

fn sentiment_label(sentiment: &str) -> &'static str {
    match sentiment {
        "positive" => "긍정",
        "negative" => "부정",
        _ => "중립",
    }
}

fn stock_summaries(points: &[NewsPerformancePoint]) -> Vec<StockPerformanceSummary> {
    let mut grouped = BTreeMap::<String, (String, Vec<(f64, bool)>)>::new();
    for point in points.iter().filter(|point| point.horizon_days == 1) {
        let Some(rate) = point.directional_return_pct else {
            continue;
        };
        grouped
            .entry(point.symbol.clone())
            .or_insert_with(|| (point.name.clone(), Vec::new()))
            .1
            .push((rate, point.hit.unwrap_or(false)));
    }
    let mut summaries = grouped
        .into_iter()
        .map(|(symbol, (name, samples))| StockPerformanceSummary {
            symbol,
            name,
            samples: samples.len() as u64,
            hit_rate_pct: samples.iter().filter(|sample| sample.1).count() as f64
                / samples.len() as f64
                * 100.0,
            average_directional_return_pct: samples.iter().map(|sample| sample.0).sum::<f64>()
                / samples.len() as f64,
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .samples
            .cmp(&left.samples)
            .then_with(|| {
                right
                    .average_directional_return_pct
                    .total_cmp(&left.average_directional_return_pct)
            })
            .then_with(|| left.symbol.cmp(&right.symbol))
    });
    summaries
}

fn sentiment_summaries(points: &[NewsPerformancePoint]) -> Vec<SentimentPerformanceSummary> {
    let mut grouped = BTreeMap::<String, Vec<(f64, bool)>>::new();
    for point in points.iter().filter(|point| point.horizon_days == 1) {
        let Some(rate) = point.directional_return_pct else {
            continue;
        };
        let Some(score) = point.sentiment_score else {
            continue;
        };
        grouped
            .entry(sentiment_key(score))
            .or_default()
            .push((rate, point.hit.unwrap_or(false)));
    }
    grouped
        .into_iter()
        .map(|(sentiment, samples)| SentimentPerformanceSummary {
            sentiment,
            samples: samples.len() as u64,
            hit_rate_pct: samples.iter().filter(|sample| sample.1).count() as f64
                / samples.len() as f64
                * 100.0,
            average_directional_return_pct: samples.iter().map(|sample| sample.0).sum::<f64>()
                / samples.len() as f64,
        })
        .collect()
}

fn maximum_drawdown(returns: impl Iterator<Item = f64>) -> Option<f64> {
    let mut equity = 1.0_f64;
    let mut peak = 1.0_f64;
    let mut max_drawdown = 0.0_f64;
    let mut count = 0;
    for rate in returns {
        equity *= 1.0 + rate / 100.0;
        peak = peak.max(equity);
        max_drawdown = max_drawdown.max((peak - equity) / peak * 100.0);
        count += 1;
    }
    (count > 0).then_some(max_drawdown)
}

fn value_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_str()?.replace(',', "").parse().ok()
}

fn open_database(state: &AppState) -> anyhow::Result<Connection> {
    let connection = Connection::open(&state.config.news_db_path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(connection)
}

fn ensure_column(connection: &Connection, name: &str, definition: &str) -> anyhow::Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(news_trade_performance)")?;
    let existing = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<HashSet<_>, _>>()?;
    if !existing.contains(name) {
        connection.execute(
            &format!("ALTER TABLE news_trade_performance ADD COLUMN {name} {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn validate_settings(settings: &NewsPerformanceSettings) -> ApiResult<()> {
    let valid = (1..=1000).contains(&settings.minimum_samples)
        && settings.minimum_hit_rate_pct.is_finite()
        && (0.0..=100.0).contains(&settings.minimum_hit_rate_pct)
        && settings.minimum_average_directional_return_pct.is_finite()
        && (-100.0..=100.0).contains(&settings.minimum_average_directional_return_pct)
        && settings.maximum_drawdown_pct.is_finite()
        && settings.maximum_drawdown_pct > 0.0
        && settings.maximum_drawdown_pct <= 100.0;
    if valid {
        Ok(())
    } else {
        Err(api_error(
            axum::http::StatusCode::BAD_REQUEST,
            "invalid_news_performance_settings",
            "Performance settings are outside the allowed range.",
        ))
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

fn database_error(
    error: impl std::fmt::Display,
) -> (axum::http::StatusCode, Json<crate::error::ApiError>) {
    api_error(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "news_performance_database_error",
        error,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn performance_point(
        symbol: &str,
        name: &str,
        directional_return_pct: f64,
        sentiment_score: Option<f64>,
    ) -> NewsPerformancePoint {
        NewsPerformancePoint {
            decision_day_key: 20261003,
            symbol: symbol.to_string(),
            name: name.to_string(),
            side: "buy".to_string(),
            confidence: 0.8,
            sentiment_score,
            horizon_days: 1,
            baseline_price: 100,
            target_at_unix: 0,
            evaluated_price: Some(100),
            raw_return_pct: Some(directional_return_pct),
            directional_return_pct: Some(directional_return_pct),
            hit: Some(directional_return_pct > 0.0),
            status: "evaluated".to_string(),
            evaluated_at_unix: Some(0),
            event_keys: Vec::new(),
        }
    }

    #[test]
    fn calculates_buy_and_sell_directional_returns() {
        let raw = calculate_return(100, 110);
        assert!((raw - 10.0).abs() < f64::EPSILON);
        assert_eq!(directional_return("buy", raw), Some(10.0));
        assert_eq!(directional_return("sell", raw), Some(-10.0));
        assert_eq!(directional_return("hold", raw), None);
    }

    #[test]
    fn calculates_maximum_drawdown_from_signal_returns() {
        let drawdown = maximum_drawdown([10.0, -20.0, 5.0].into_iter()).unwrap_or_default();
        assert!((drawdown - 20.0).abs() < 0.001);
    }

    #[test]
    fn summarizes_one_day_results_by_stock_and_sentiment() {
        let points = vec![
            performance_point("005930", "삼성전자", 4.0, Some(0.6)),
            performance_point("005930", "삼성전자", -2.0, Some(-0.4)),
            performance_point("000660", "SK하이닉스", 3.0, Some(0.0)),
        ];

        let stocks = stock_summaries(&points);
        assert_eq!(stocks.len(), 2);
        assert_eq!(stocks[0].symbol, "005930");
        assert_eq!(stocks[0].samples, 2);
        assert!((stocks[0].hit_rate_pct - 50.0).abs() < f64::EPSILON);
        assert!((stocks[0].average_directional_return_pct - 1.0).abs() < f64::EPSILON);

        let sentiments = sentiment_summaries(&points);
        assert_eq!(sentiments.len(), 3);
        assert!(sentiments
            .iter()
            .any(|summary| { summary.sentiment == "positive" && summary.hit_rate_pct == 100.0 }));
        assert!(sentiments
            .iter()
            .any(|summary| { summary.sentiment == "negative" && summary.hit_rate_pct == 0.0 }));
        assert!(sentiments
            .iter()
            .any(|summary| { summary.sentiment == "neutral" && summary.hit_rate_pct == 100.0 }));
    }

    #[test]
    fn uses_conservative_recommended_exclusion_defaults() {
        let settings = NewsPerformanceSettings::default();
        assert!(settings.enabled);
        assert_eq!(settings.minimum_samples, 20);
        assert_eq!(settings.minimum_hit_rate_pct, 45.0);
        assert_eq!(settings.minimum_average_directional_return_pct, 0.0);
        assert_eq!(settings.maximum_drawdown_pct, 10.0);
        assert!(validate_settings(&settings).is_ok());
    }

    #[test]
    fn excludes_only_after_minimum_sample_count() {
        let settings = NewsPerformanceSettings::default();
        let insufficient = SignalMetrics {
            samples: 19,
            hit_rate_pct: 0.0,
            average_directional_return_pct: -10.0,
            maximum_drawdown_pct: 50.0,
        };
        assert!(metrics_exclusion_reason(&settings, "삼성전자", &insufficient).is_none());

        let poor = SignalMetrics {
            samples: 20,
            hit_rate_pct: 40.0,
            average_directional_return_pct: -1.0,
            maximum_drawdown_pct: 12.0,
        };
        let reason = metrics_exclusion_reason(&settings, "삼성전자", &poor);
        assert!(reason.unwrap_or_default().contains("적중률"));
    }
}
