use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::AppState;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PricePlan {
    pub symbol: String,
    pub action: String,
    pub confidence: f64,
    pub entry_price: u64,
    pub take_profit_price: u64,
    pub stop_loss_price: u64,
    pub quantity: u32,
    pub valid_for_minutes: u64,
    pub reason: String,
    #[serde(default = "regular_session_name")]
    pub session: String,
}

fn regular_session_name() -> String {
    "regular".to_string()
}

#[derive(Clone, Deserialize, Serialize)]
pub struct StoredPlan {
    pub plan: PricePlan,
    pub model: String,
    pub generated_at_unix: u64,
    pub expires_at_unix: u64,
    pub context: Value,
    pub submitted_side: Option<String>,
    pub submitted_quantity: u32,
    pub holding_before_order: u32,
    pub order_status: String,
    #[serde(default)]
    pub broker_order: Option<crate::broker_orders::TrackedOrder>,
}

#[derive(Deserialize)]
struct PlanResponse {
    model: String,
    plan: PricePlan,
    usage: Option<crate::strategy::NewsAnalysisUsage>,
}

pub fn initialize(state: &AppState) -> anyhow::Result<()> {
    connection(state)?.execute_batch(
        "CREATE TABLE IF NOT EXISTS ai_price_plans (
            symbol TEXT PRIMARY KEY, record_json TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS ai_price_plan_calls (
            id INTEGER PRIMARY KEY, started_at_unix INTEGER NOT NULL,
            status TEXT NOT NULL, usage_json TEXT, error TEXT
         );
         CREATE TABLE IF NOT EXISTS ai_daily_equity (
            day_key INTEGER PRIMARY KEY, opening_equity INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS ai_order_events (
            id INTEGER PRIMARY KEY, timestamp_unix INTEGER NOT NULL,
            symbol TEXT NOT NULL, event TEXT NOT NULL, record_json TEXT NOT NULL
         );",
    )?;
    Ok(())
}

pub fn daily_loss_blocked(
    state: &AppState,
    equity: u64,
    now: u64,
    limit: f64,
) -> anyhow::Result<bool> {
    anyhow::ensure!(equity > 0, "Account equity is unavailable");
    let db = connection(state)?;
    let day_key = (now + 9 * 3600) / 86_400;
    db.execute(
        "INSERT OR IGNORE INTO ai_daily_equity(day_key,opening_equity) VALUES (?1,?2)",
        params![day_key, equity],
    )?;
    let opening: u64 = db.query_row(
        "SELECT opening_equity FROM ai_daily_equity WHERE day_key=?1",
        [day_key],
        |row| row.get(0),
    )?;
    Ok((equity as f64) <= opening as f64 * (1.0 - limit))
}

fn connection(state: &AppState) -> anyhow::Result<Connection> {
    let db = Connection::open(&state.config.news_db_path)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(db)
}

pub fn load(state: &AppState, symbol: &str) -> anyhow::Result<Option<StoredPlan>> {
    let raw: Option<String> = connection(state)?
        .query_row(
            "SELECT record_json FROM ai_price_plans WHERE symbol = ?1",
            [symbol],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(Into::into))
        .transpose()
}

pub fn save(state: &AppState, record: &StoredPlan) -> anyhow::Result<()> {
    connection(state)?.execute(
        "INSERT INTO ai_price_plans(symbol, record_json) VALUES (?1, ?2)
         ON CONFLICT(symbol) DO UPDATE SET record_json = excluded.record_json",
        params![record.plan.symbol, serde_json::to_string(record)?],
    )?;
    Ok(())
}

pub fn list(state: &AppState) -> anyhow::Result<Vec<StoredPlan>> {
    let db = connection(state)?;
    let mut statement = db.prepare("SELECT record_json FROM ai_price_plans ORDER BY symbol")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
}

pub async fn generate(state: &AppState, context: Value, now: u64) -> anyhow::Result<StoredPlan> {
    let record = propose(state, context, now).await?;
    save(state, &record)?;
    Ok(record)
}

pub async fn propose(state: &AppState, context: Value, now: u64) -> anyhow::Result<StoredPlan> {
    let db = connection(state)?;
    let day_start = ((now + 9 * 3600) / 86_400) * 86_400 - 9 * 3600;
    // Count attempts before the network call, including failures and process restarts.
    let attempts: u64 = db.query_row(
        "SELECT COUNT(*) FROM ai_price_plan_calls WHERE started_at_unix >= ?1",
        [day_start],
        |r| r.get(0),
    )?;
    anyhow::ensure!(
        attempts < state.config.ai_plan_daily_limit,
        "AI price plan daily call limit reached"
    );
    db.execute(
        "INSERT INTO ai_price_plan_calls(started_at_unix,status) VALUES (?1,'started')",
        [now],
    )?;
    let call_id = db.last_insert_rowid();
    drop(db);
    let result = async {
        let response = state
            .http
            .post(format!("{}/strategy/trade-plan", state.config.strategy_url))
            .timeout(std::time::Duration::from_secs(55))
            .json(&context)
            .send()
            .await?
            .error_for_status()?
            .json::<PlanResponse>()
            .await?;
        validate(&response.plan, &context)?;
        connection(state)?.execute(
            "UPDATE ai_price_plan_calls SET status='success',usage_json=?1 WHERE id=?2",
            params![serde_json::to_string(&response.usage)?, call_id],
        )?;
        Ok::<_, anyhow::Error>(StoredPlan {
            expires_at_unix: now + response.plan.valid_for_minutes * 60,
            plan: response.plan,
            model: response.model,
            generated_at_unix: now,
            context,
            submitted_side: None,
            submitted_quantity: 0,
            holding_before_order: 0,
            order_status: "watching".to_string(),
            broker_order: None,
        })
    }
    .await;
    if let Err(error) = &result {
        connection(state)?.execute(
            "UPDATE ai_price_plan_calls SET status='failed',error=?1 WHERE id=?2",
            params![error.to_string(), call_id],
        )?;
    }
    let record = result?;
    Ok(record)
}

pub fn record_order_event(
    state: &AppState,
    record: &StoredPlan,
    event: &str,
    now: u64,
) -> anyhow::Result<()> {
    let mut db = connection(state)?;
    let transaction = db.transaction()?;
    let raw = serde_json::to_string(record)?;
    transaction.execute(
        "INSERT INTO ai_price_plans(symbol,record_json) VALUES (?1,?2)
        ON CONFLICT(symbol) DO UPDATE SET record_json=excluded.record_json",
        params![record.plan.symbol, raw],
    )?;
    transaction.execute(
        "INSERT INTO ai_order_events(timestamp_unix,symbol,event,record_json) VALUES (?1,?2,?3,?4)",
        params![now, record.plan.symbol, event, raw],
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn validate(plan: &PricePlan, context: &Value) -> anyhow::Result<()> {
    anyhow::ensure!(
        matches!(plan.session.as_str(), "regular" | "opening_auction"),
        "Unsupported trading session"
    );
    anyhow::ensure!(
        context["symbol"].as_str() == Some(&plan.symbol),
        "AI returned a different symbol"
    );
    anyhow::ensure!(
        plan.confidence.is_finite() && (0.0..=1.0).contains(&plan.confidence),
        "Invalid confidence"
    );
    anyhow::ensure!(
        (5..=1440).contains(&plan.valid_for_minutes),
        "Invalid plan lifetime"
    );
    let held = context["holding_quantity"].as_u64().unwrap_or(0);
    if plan.stop_loss_price > 0 || plan.take_profit_price > 0 {
        anyhow::ensure!(
            plan.stop_loss_price > 0 && plan.stop_loss_price < plan.take_profit_price,
            "Invalid protective exit prices"
        );
    }
    let amount = plan
        .entry_price
        .checked_mul(u64::from(plan.quantity))
        .ok_or_else(|| anyhow::anyhow!("Order amount overflow"))?;
    match plan.action.as_str() {
        "buy" => {
            anyhow::ensure!(
                !context["news"].is_null(),
                "Confirmed news is required for a new buy"
            );
            anyhow::ensure!(
                (held == 0 || context["pending_order"]["side"].as_str() == Some("buy"))
                    && plan.quantity > 0
                    && plan.stop_loss_price > 0
                    && plan.stop_loss_price < plan.entry_price
                    && plan.entry_price < plan.take_profit_price,
                "Invalid buy price plan"
            );
            anyhow::ensure!(
                amount <= context["maximum_buy_amount_krw"].as_u64().unwrap_or(0),
                "Buy budget exceeded"
            );
        }
        "sell" => anyhow::ensure!(
            plan.entry_price > 0 && plan.quantity > 0 && u64::from(plan.quantity) <= held,
            "Sell exceeds holdings"
        ),
        "hold" => anyhow::ensure!(plan.quantity == 0, "Hold cannot order shares"),
        _ => anyhow::bail!("Unknown AI action"),
    }
    anyhow::ensure!(
        amount <= context["maximum_order_amount_krw"].as_u64().unwrap_or(0),
        "Order limit exceeded"
    );
    if let Some(remaining) = context["pending_order"]["remaining_quantity"].as_u64() {
        anyhow::ensure!(
            u64::from(plan.quantity) <= remaining,
            "Replacement exceeds unfilled quantity"
        );
    }
    if held > 0
        && context["previous_plan"]["stop_loss_price"]
            .as_u64()
            .unwrap_or(0)
            > 0
    {
        anyhow::ensure!(
            plan.stop_loss_price > 0 && plan.stop_loss_price < plan.take_profit_price,
            "Protective exit prices are required"
        );
    }
    Ok(())
}

pub fn trigger(plan: &PricePlan, price: u64, held: u32) -> Option<&'static str> {
    if held > 0 {
        if (plan.stop_loss_price > 0 && price <= plan.stop_loss_price)
            || (plan.take_profit_price > 0 && price >= plan.take_profit_price)
            || (plan.action == "sell" && price >= plan.entry_price)
        {
            return Some("sell");
        }
    } else if plan.action == "buy" && price <= plan.entry_price && price > plan.stop_loss_price {
        return Some("buy");
    }
    None
}

pub fn regular_session(now: u64) -> bool {
    let kst = now + 9 * 3600;
    let weekday = (kst / 86_400 + 3) % 7 < 5;
    let minute = kst % 86_400 / 60;
    weekday && (9 * 60..15 * 60 + 20).contains(&minute)
}

pub fn opening_auction(now: u64) -> bool {
    let kst = now + 9 * 3600;
    (kst / 86_400 + 3) % 7 < 5 && (8 * 60 + 30..9 * 60).contains(&(kst % 86_400 / 60))
}

pub fn order_session_allowed(plan: &PricePlan, now: u64) -> bool {
    regular_session(now) || (plan.session == "opening_auction" && opening_auction(now))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn generates_persists_and_caps_ai_calls_after_restart() {
        let app = axum::Router::new().route(
            "/strategy/trade-plan",
            axum::routing::post(|| async {
                axum::Json(
                    serde_json::json!({"model":"mock", "plan": plan(), "usage": {
                        "input_tokens":100, "output_tokens":50, "total_tokens":150
                    }}),
                )
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let path = std::env::temp_dir().join(format!(
            "ai-plan-storage-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = crate::config::AppConfig::load();
        config.news_db_path = path.to_string_lossy().into_owned();
        config.strategy_url = format!("http://{address}");
        config.ai_plan_daily_limit = 1;
        let state = AppState::new(config.clone());
        initialize(&state).unwrap();
        let now = 1_800_000_000;
        let context = serde_json::json!({"symbol":"005930", "holding_quantity":0,
            "maximum_buy_amount_krw":100_000, "maximum_order_amount_krw":100_000,
            "news": {"reasons":["confirmed news"]}});
        let generated = generate(&state, context.clone(), now).await.unwrap();
        assert_eq!(generated.plan.entry_price, 70_000);
        let restarted = AppState::new(config);
        assert_eq!(load(&restarted, "005930").unwrap().unwrap().model, "mock");
        assert!(generate(&restarted, context, now + 60)
            .await
            .err()
            .expect("daily limit error")
            .to_string()
            .contains("daily call limit"));
        assert!(!daily_loss_blocked(&state, 500_000, now, 0.03).unwrap());
        assert!(daily_loss_blocked(&restarted, 480_000, now + 60, 0.03).unwrap());
        assert!(!daily_loss_blocked(&restarted, 480_000, now + 86_400, 0.03).unwrap());
        server.abort();
        let _ = server.await;
        std::fs::remove_file(path).unwrap();
    }

    fn plan() -> PricePlan {
        PricePlan {
            symbol: "005930".into(),
            action: "buy".into(),
            confidence: 0.8,
            entry_price: 70_000,
            take_profit_price: 75_000,
            stop_loss_price: 65_000,
            quantity: 1,
            valid_for_minutes: 60,
            reason: "test".into(),
            session: "regular".into(),
        }
    }

    #[test]
    fn triggers_entry_and_protective_exits_without_buying_below_stop() {
        let plan = plan();
        assert_eq!(trigger(&plan, 71_000, 0), None);
        assert_eq!(trigger(&plan, 70_000, 0), Some("buy"));
        assert_eq!(trigger(&plan, 64_000, 0), None);
        assert_eq!(trigger(&plan, 75_000, 1), Some("sell"));
        assert_eq!(trigger(&plan, 64_000, 1), Some("sell"));
        assert_eq!(trigger(&plan, 70_000, 1), None);
    }

    #[test]
    fn rejects_invalid_model_prices_symbols_and_budgets() {
        let mut plan = plan();
        let context = serde_json::json!({"symbol":"005930", "holding_quantity":0,
            "maximum_buy_amount_krw":100_000, "maximum_order_amount_krw":100_000,
            "news": {"reasons": ["confirmed news"]}});
        assert!(validate(&plan, &context).is_ok());
        plan.quantity = 2;
        assert!(validate(&plan, &context).is_err());
        plan.quantity = 1;
        plan.stop_loss_price = 71_000;
        assert!(validate(&plan, &context).is_err());
        plan = self::plan();
        plan.symbol = "000660".into();
        assert!(validate(&plan, &context).is_err());
    }

    #[test]
    fn session_gate_uses_korea_time_and_weekdays() {
        let friday = 20_000 * 86_400 - 9 * 3600;
        assert!(!regular_session(friday + 8 * 3600));
        assert!(regular_session(friday + 9 * 3600));
        assert!(!regular_session(friday + 15 * 3600 + 20 * 60));
        assert!(!regular_session(friday + 86_400 + 10 * 3600));
    }
}
