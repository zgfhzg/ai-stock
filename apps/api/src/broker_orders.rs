use axum::Json;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    ai_plans::{self, PricePlan, StoredPlan},
    kis,
    risk_settings::RiskSettings,
    state::AppState,
};

#[derive(Clone, Deserialize, Serialize)]
pub struct TrackedOrder {
    #[serde(default)]
    pub side: String,
    pub order_no: String,
    pub branch_no: String,
    pub order_date: String,
    pub submitted_at_unix: u64,
    pub price: u64,
    pub quantity: u32,
    pub filled_quantity: u32,
    #[serde(default)]
    pub filled_amount_krw: Option<f64>,
    pub remaining_quantity: u32,
    pub last_review_at_unix: u64,
    pub cancel_requested: bool,
    pub replacement: Option<PricePlan>,
    pub replacement_model: Option<String>,
    pub replacement_expires_at_unix: Option<u64>,
    #[serde(default)]
    pub replacement_context: Option<Value>,
    pub reason: String,
}

#[derive(Debug)]
struct BrokerStatus {
    order_no: String,
    branch_no: String,
    filled: u32,
    filled_amount: Option<f64>,
    remaining: u32,
    closed: bool,
}

fn date_at(now: u64) -> anyhow::Result<String> {
    Ok(Connection::open_in_memory()?.query_row(
        "SELECT strftime('%Y%m%d', ?1, 'unixepoch', '+9 hours')",
        [now],
        |row| row.get(0),
    )?)
}

pub fn pending(side: &str, price: u64, quantity: u32, now: u64) -> anyhow::Result<TrackedOrder> {
    Ok(TrackedOrder {
        side: side.into(),
        order_no: String::new(),
        branch_no: String::new(),
        order_date: date_at(now)?,
        submitted_at_unix: now,
        price,
        quantity,
        filled_quantity: 0,
        filled_amount_krw: None,
        remaining_quantity: quantity,
        last_review_at_unix: now,
        cancel_requested: false,
        replacement: None,
        replacement_model: None,
        replacement_expires_at_unix: None,
        replacement_context: None,
        reason: "주문 접수 결과 확인 중".into(),
    })
}

pub fn capture_identity(order: &mut TrackedOrder, output: Option<&Value>) {
    if let Some(output) = output {
        order.order_no = text(output, &["ODNO", "odno"]).unwrap_or_default();
        order.branch_no = text(
            output,
            &["KRX_FWDG_ORD_ORGNO", "krx_fwdg_ord_orgno", "ord_gno_brno"],
        )
        .unwrap_or_default();
    }
}

fn hydrate_legacy(state: &AppState, record: &mut StoredPlan) -> anyhow::Result<()> {
    if record.broker_order.is_some() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&state.config.order_log_path)?;
    let mut matches = Vec::new();
    for line in content.lines() {
        let row: Value = serde_json::from_str(line)?;
        if row["timestamp_unix"]
            .as_u64()
            .is_some_and(|time| time >= record.generated_at_unix)
            && row["request"]["symbol"].as_str() == Some(&record.plan.symbol)
            && row["request"]["side"].as_str() == record.submitted_side.as_deref()
            && row["request"]["quantity"].as_u64() == Some(u64::from(record.submitted_quantity))
            && row["response"]["accepted"].as_bool() == Some(true)
        {
            matches.push(row);
        }
    }
    anyhow::ensure!(
        matches.len() == 1,
        "Legacy order receipt is missing or ambiguous"
    );
    let receipt = &matches[0];
    let mut tracked = pending(
        record.submitted_side.as_deref().unwrap_or(""),
        receipt["request"]["price"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing receipt price"))?,
        record.submitted_quantity,
        receipt["timestamp_unix"].as_u64().unwrap(),
    )?;
    capture_identity(
        &mut tracked,
        receipt["response"]["kis"]["output"]
            .as_object()
            .map(|_| &receipt["response"]["kis"]["output"]),
    );
    anyhow::ensure!(
        !tracked.order_no.is_empty(),
        "Legacy receipt has no broker order number"
    );
    record.broker_order = Some(tracked);
    ai_plans::save(state, record)?;
    Ok(())
}

fn text(row: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        row.get(*name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn number(row: &Value, name: &str) -> anyhow::Result<u32> {
    let raw = row
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Missing broker field: {name}"))?;
    let value = raw
        .as_str()
        .map(|s| s.trim().replace(',', ""))
        .unwrap_or_else(|| raw.to_string());
    Ok(value.parse()?)
}

async fn order_rows(state: &AppState, record: &StoredPlan, now: u64) -> anyhow::Result<Vec<Value>> {
    let tracked = record
        .broker_order
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Legacy order has no broker identity"))?;
    let end = date_at(now)?;
    let side = if record.submitted_side.as_deref() == Some("buy") {
        "02"
    } else {
        "01"
    };
    let tr_id = if state.config.trading_mode == "paper" {
        "VTTC0081R"
    } else {
        "TTTC0081R"
    };
    let (mut fk, mut nk) = (String::new(), String::new());
    let mut rows = Vec::new();
    for page in 0..20 {
        let params = [
            ("CANO", state.config.kis_account_no.as_str()),
            (
                "ACNT_PRDT_CD",
                state.config.kis_account_product_code.as_str(),
            ),
            ("INQR_STRT_DT", tracked.order_date.as_str()),
            ("INQR_END_DT", end.as_str()),
            ("SLL_BUY_DVSN_CD", side),
            ("PDNO", record.plan.symbol.as_str()),
            ("CCLD_DVSN", "00"),
            ("INQR_DVSN", "00"),
            ("INQR_DVSN_3", "00"),
            ("ORD_GNO_BRNO", ""),
            ("ODNO", tracked.order_no.as_str()),
            ("INQR_DVSN_1", ""),
            ("CTX_AREA_FK100", fk.as_str()),
            ("CTX_AREA_NK100", nk.as_str()),
            ("EXCG_ID_DVSN_CD", "KRX"),
        ];
        let response = kis::kis_get_page(
            state,
            "/uapi/domestic-stock/v1/trading/inquire-daily-ccld",
            tr_id,
            &params,
            page > 0,
        )
        .await
        .map_err(|(_, Json(error))| anyhow::anyhow!(error.message))?;
        anyhow::ensure!(
            response["rt_cd"].as_str() == Some("0"),
            "Broker query was not successful"
        );
        rows.extend(
            response["output1"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Missing broker order rows"))?
                .iter()
                .cloned(),
        );
        if response["_has_more"].as_bool() != Some(true) {
            return Ok(rows);
        }
        let next_fk = response["ctx_area_fk100"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        let next_nk = response["ctx_area_nk100"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        anyhow::ensure!(
            (next_fk != fk || next_nk != nk) && !next_nk.is_empty(),
            "Invalid broker pagination cursor"
        );
        (fk, nk) = (next_fk, next_nk);
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    }
    anyhow::bail!("Broker order query exceeded pagination limit")
}

fn match_status(record: &StoredPlan, rows: &[Value]) -> anyhow::Result<BrokerStatus> {
    let tracked = record
        .broker_order
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Order identity unavailable"))?;
    let side = if record.submitted_side.as_deref() == Some("buy") {
        "02"
    } else {
        "01"
    };
    let matching = rows
        .iter()
        .filter(|row| {
            if row["pdno"].as_str() != Some(&record.plan.symbol)
                || row["ord_dt"].as_str() != Some(&tracked.order_date)
                || row["sll_buy_dvsn_cd"].as_str() != Some(side)
            {
                return false;
            }
            if !tracked.order_no.is_empty() {
                return row["odno"].as_str() == Some(&tracked.order_no);
            }
            let time = row["ord_tmd"].as_str().and_then(|s| s.parse::<u64>().ok());
            let seconds = time.map(|t| (t / 10000) * 3600 + (t / 100 % 100) * 60 + t % 100);
            let submitted = (tracked.submitted_at_unix + 9 * 3600) % 86_400;
            number(row, "ord_qty").ok() == Some(tracked.quantity)
                && row["ord_unpr"].as_str().and_then(|s| s.parse::<u64>().ok())
                    == Some(tracked.price)
                && seconds.is_some_and(|s| s.abs_diff(submitted) <= 120)
        })
        .collect::<Vec<_>>();
    anyhow::ensure!(
        matching.len() == 1,
        "Order identity is missing or ambiguous; waiting for reconciliation"
    );
    let row = matching[0];
    let ordered = number(row, "ord_qty")?;
    let filled = number(row, "tot_ccld_qty")?;
    let remaining = number(row, "rmn_qty")?;
    anyhow::ensure!(
        ordered == tracked.quantity
            && filled <= ordered
            && remaining <= ordered
            && filled.saturating_add(remaining) <= ordered,
        "Inconsistent broker order quantities"
    );
    let closed = row["cncl_yn"].as_str() == Some("Y")
        || filled == ordered
        || number(row, "rjct_qty").ok() == Some(ordered);
    anyhow::ensure!(
        closed || remaining > 0,
        "Broker order state is not yet final"
    );
    Ok(BrokerStatus {
        order_no: text(row, &["odno"]).ok_or_else(|| anyhow::anyhow!("Missing order number"))?,
        branch_no: text(row, &["ord_gno_brno", "krx_fwdg_ord_orgno"])
            .unwrap_or_else(|| tracked.branch_no.clone()),
        filled,
        filled_amount: row["tot_ccld_amt"]
            .as_str()
            .and_then(|v| v.replace(',', "").parse::<f64>().ok())
            .or_else(|| row["tot_ccld_amt"].as_f64())
            .filter(|v| v.is_finite() && *v > 0.0),
        remaining: if closed { 0 } else { remaining },
        closed,
    })
}

async fn cancel_remaining(state: &AppState, tracked: &TrackedOrder) -> anyhow::Result<()> {
    anyhow::ensure!(
        !tracked.order_no.is_empty()
            && !tracked.branch_no.is_empty()
            && tracked.remaining_quantity > 0,
        "Cannot cancel without broker identity and confirmed remaining quantity"
    );
    let body = json!({"CANO":state.config.kis_account_no,"ACNT_PRDT_CD":state.config.kis_account_product_code,
        "KRX_FWDG_ORD_ORGNO":tracked.branch_no,"ORGN_ODNO":tracked.order_no,"ORD_DVSN":"00",
        "RVSE_CNCL_DVSN_CD":"02","ORD_QTY":tracked.remaining_quantity.to_string(),"ORD_UNPR":"0",
        "QTY_ALL_ORD_YN":"Y","EXCG_ID_DVSN_CD":"KRX"});
    let tr_id = if state.config.trading_mode == "paper" {
        "VTTC0013U"
    } else {
        "TTTC0013U"
    };
    let response = kis::kis_post(
        state,
        "/uapi/domestic-stock/v1/trading/order-rvsecncl",
        tr_id,
        body,
    )
    .await
    .map_err(|(_, Json(error))| anyhow::anyhow!(error.message))?;
    anyhow::ensure!(
        response["rt_cd"].as_str() == Some("0"),
        "Cancel request not accepted"
    );
    Ok(())
}

/// A terminal broker result is required before releasing the symbol for another order.
pub async fn manage(
    state: &AppState,
    record: &mut StoredPlan,
    position: (u32, u64),
    execute: bool,
    risk: &RiskSettings,
    available_budget: u64,
    now: u64,
) -> anyhow::Result<()> {
    let (held, holding_value) = position;
    hydrate_legacy(state, record)?;
    let rows = order_rows(state, record, now).await?;
    let status = match_status(record, &rows)?;
    let side = record
        .submitted_side
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Missing submitted side"))?;
    let tracked = record.broker_order.as_mut().unwrap();
    let changed = tracked.filled_quantity != status.filled
        || tracked.filled_amount_krw != status.filled_amount
        || tracked.remaining_quantity != status.remaining
        || tracked.order_no != status.order_no;
    tracked.order_no = status.order_no;
    tracked.branch_no = status.branch_no;
    tracked.filled_quantity = status.filled;
    tracked.filled_amount_krw = status.filled_amount;
    tracked.remaining_quantity = status.remaining;
    if status.closed {
        let balance_matches = if side == "buy" {
            held >= record.holding_before_order.saturating_add(status.filled)
        } else {
            held <= record.holding_before_order.saturating_sub(status.filled)
        };
        if !balance_matches {
            record.order_status = "filled_waiting_balance".into();
            tracked.reason = "체결 조회 완료, 잔고 반영 대기".into();
        } else {
            record.submitted_side = None;
            record.order_status = if status.filled == tracked.quantity {
                "filled"
            } else {
                "cancelled"
            }
            .into();
            tracked.reason = if status.filled == tracked.quantity {
                "체결 완료"
            } else {
                "미체결 잔량 취소 확인"
            }
            .into();
            if status.filled == 0 {
                if let Some(replacement) = tracked.replacement.take().filter(|_| {
                    tracked
                        .replacement_expires_at_unix
                        .is_some_and(|until| until > now)
                }) {
                    record.plan = replacement;
                    record.model = tracked
                        .replacement_model
                        .take()
                        .unwrap_or_else(|| record.model.clone());
                    record.expires_at_unix = tracked.replacement_expires_at_unix.unwrap();
                    record.generated_at_unix = tracked.last_review_at_unix;
                    if let Some(context) = tracked.replacement_context.take() {
                        record.context = context;
                    }
                    record.order_status = "replacement_ready".into();
                    tracked.reason = "취소 확정, AI 변경 가격으로 재주문 준비".into();
                } else if held == 0 {
                    record.plan.action = "hold".into();
                    record.plan.quantity = 0;
                    record.expires_at_unix = now + 300;
                }
            } else if held == 0 {
                record.plan.action = "hold".into();
                record.plan.quantity = 0;
                record.expires_at_unix = now + 300;
            }
        }
        ai_plans::record_order_event(state, record, "broker_terminal", now)?;
        return Ok(());
    }
    record.order_status = if status.filled > 0 {
        "partially_filled"
    } else {
        "unfilled"
    }
    .into();
    record.broker_order.as_mut().unwrap().reason = if status.filled > 0 {
        "부분 체결, 남은 주문 관리 중"
    } else {
        "미체결 주문 감시 중"
    }
    .into();
    if changed {
        ai_plans::record_order_event(state, record, "broker_update", now)?;
    }
    if !execute {
        ai_plans::save(state, record)?;
        return Ok(());
    }
    anyhow::ensure!(
        state.config.trading_mode == "paper",
        "Automatic order management requires paper mode"
    );
    let kst = now + 9 * 3600;
    let minute = kst % 86_400 / 60;
    let managed_session =
        (kst / 86_400 + 3) % 7 < 5 && (8 * 60 + 30..15 * 60 + 30).contains(&minute);
    if !managed_session {
        ai_plans::save(state, record)?;
        return Ok(());
    }
    if record.broker_order.as_ref().unwrap().cancel_requested {
        record.order_status = "cancel_pending".into();
        record.broker_order.as_mut().unwrap().reason = "취소 요청 후 증권사 확정 대기".into();
        if now.saturating_sub(record.broker_order.as_ref().unwrap().last_review_at_unix) >= 60 {
            record.broker_order.as_mut().unwrap().last_review_at_unix = now;
            ai_plans::record_order_event(state, record, "cancel_recheck", now)?;
            if let Err(error) = cancel_remaining(state, record.broker_order.as_ref().unwrap()).await
            {
                record.broker_order.as_mut().unwrap().reason = format!("취소 확인 재시도: {error}");
            }
        }
        ai_plans::save(state, record)?;
        return Ok(());
    }
    let protective_buy_cancel = side == "buy"
        && record.plan.stop_loss_price > 0
        && held > record.holding_before_order
        && holding_value / u64::from(held.max(1)) <= record.plan.stop_loss_price;
    let expired = record.expires_at_unix <= now
        || minute >= 15 * 60 + 20
        || protective_buy_cancel
        || record.broker_order.as_ref().unwrap().order_date != date_at(now)?;
    let mut cancel = expired;
    if !expired
        && now.saturating_sub(record.broker_order.as_ref().unwrap().last_review_at_unix)
            >= state.config.ai_order_review_seconds
    {
        let quote = kis::get_price(state, &record.plan.symbol)
            .await
            .map_err(|(_, Json(error))| anyhow::anyhow!(error.message))?;
        let output = quote
            .output
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing reprice quote"))?;
        let price = output["stck_prpr"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|p| *p > 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid reprice quote"))?;
        let tracked = record.broker_order.as_ref().unwrap();
        let remaining = tracked.remaining_quantity;
        let room = ((risk.auto_trading_budget_krw as f64 * risk.max_position_ratio) as u64)
            .saturating_sub(holding_value);
        let used = crate::auto_trading::today_buy_amount(state, &record.plan.symbol, now)
            .map_err(|(_, Json(error))| anyhow::anyhow!(error.message))?;
        let reserved = tracked.price.saturating_mul(u64::from(remaining));
        let daily_room = risk
            .max_daily_auto_order_amount_krw_per_symbol
            .saturating_sub(used)
            .saturating_add(reserved);
        let fresh_news = crate::news::daily_outlooks(state)
            .map_err(|(_, Json(error))| anyhow::anyhow!(error.message))?
            .into_iter()
            .find(|outlook| outlook.symbol == record.plan.symbol);
        let context = json!({"symbol":record.plan.symbol,"name":record.context["name"],"current_price":price,
            "quote":output,"news":fresh_news.map(|news| serde_json::to_value(news).unwrap_or(Value::Null)).unwrap_or_else(|| record.context["news"].clone()),"holding_quantity":held,
            "average_purchase_price":record.context["average_purchase_price"],
            "maximum_buy_amount_krw":available_budget.saturating_add(reserved).min(room).min(risk.max_order_amount_krw).min(daily_room),
            "maximum_order_amount_krw":risk.max_order_amount_krw,"previous_plan":record.plan,
            "pending_order":{"side":side,"remaining_quantity":remaining,"filled_quantity":status.filled},
            "available_sessions":["regular","opening_auction"]});
        let reviewed = ai_plans::propose(state, context, now).await;
        record.broker_order.as_mut().unwrap().last_review_at_unix = now;
        match reviewed {
            Ok(reviewed) => {
                let reprice = reviewed.plan.action == side
                    && (reviewed.plan.entry_price != tracked_price(record)
                        || reviewed.plan.quantity < remaining);
                cancel = reviewed.plan.action != side || reprice;
                let tracked = record.broker_order.as_mut().unwrap();
                tracked.reason = reviewed.plan.reason.clone();
                if reprice {
                    tracked.replacement = Some(reviewed.plan);
                    tracked.replacement_model = Some(reviewed.model);
                    tracked.replacement_expires_at_unix = Some(reviewed.expires_at_unix);
                    tracked.replacement_context = Some(reviewed.context);
                } else if !cancel {
                    record.plan.take_profit_price = reviewed.plan.take_profit_price;
                    record.plan.stop_loss_price = reviewed.plan.stop_loss_price;
                    record.plan.confidence = reviewed.plan.confidence;
                    record.plan.reason = reviewed.plan.reason;
                }
            }
            Err(error) => {
                record.broker_order.as_mut().unwrap().reason = format!("AI 재평가 대기: {error}")
            }
        }
        ai_plans::record_order_event(state, record, "ai_order_review", now)?;
    }
    if cancel {
        record.broker_order.as_mut().unwrap().cancel_requested = true;
        record.broker_order.as_mut().unwrap().last_review_at_unix = now;
        record.order_status = "cancel_pending".into();
        ai_plans::record_order_event(state, record, "cancel_intent", now)?;
        let outcome = cancel_remaining(state, record.broker_order.as_ref().unwrap()).await;
        if let Err(error) = outcome {
            // Do not resubmit or free the reservation after an uncertain mutation response.
            record.broker_order.as_mut().unwrap().reason = format!("취소 결과 확인 대기: {error}");
        }
        ai_plans::record_order_event(state, record, "cancel_requested", now)?;
    } else {
        ai_plans::save(state, record)?;
    }
    Ok(())
}

fn tracked_price(record: &StoredPlan) -> u64 {
    record.broker_order.as_ref().unwrap().price
}

pub fn update_buy_commitments(
    state: &AppState,
    symbol: &str,
    day_start: u64,
    amounts: &mut std::collections::HashMap<String, u64>,
) -> anyhow::Result<()> {
    let db = Connection::open(&state.config.news_db_path)?;
    let mut statement = db.prepare("SELECT record_json FROM ai_order_events WHERE symbol=?1 AND timestamp_unix>=?2 ORDER BY id")?;
    let rows = statement.query_map(rusqlite::params![symbol, day_start], |row| {
        row.get::<_, String>(0)
    })?;
    for row in rows {
        let record: StoredPlan = serde_json::from_str(&row?)?;
        let Some(order) = record.broker_order else {
            continue;
        };
        if order.side != "buy" || order.order_no.is_empty() || order.submitted_at_unix < day_start {
            continue;
        }
        let quantity = order
            .filled_quantity
            .saturating_add(order.remaining_quantity);
        let key = format!("{}:{}", order.order_date, order.order_no);
        amounts.insert(key, order.price.saturating_mul(u64::from(quantity)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::Query,
        routing::{get, post},
        Router,
    };
    use std::sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    };

    fn record(now: u64) -> StoredPlan {
        let mut tracked = pending("buy", 10_000, 3, now - 600).unwrap();
        tracked.order_no = "001".into();
        tracked.branch_no = "06010".into();
        StoredPlan {
            plan: PricePlan {
                symbol: "005930".into(),
                action: "buy".into(),
                confidence: 0.9,
                entry_price: 10_000,
                take_profit_price: 11_000,
                stop_loss_price: 9_000,
                quantity: 3,
                valid_for_minutes: 60,
                reason: "test".into(),
                session: "regular".into(),
            },
            model: "mock".into(),
            generated_at_unix: now - 600,
            expires_at_unix: now + 3600,
            context: json!({"name":"test","news":{"reasons":["confirmed news"]},"average_purchase_price":10000}),
            submitted_side: Some("buy".into()),
            submitted_quantity: 3,
            holding_before_order: 0,
            order_status: "accepted".into(),
            broker_order: Some(tracked),
        }
    }

    fn row(record: &StoredPlan, filled: u32, remaining: u32, cancelled: bool) -> Value {
        json!({"pdno":"005930","ord_dt":record.broker_order.as_ref().unwrap().order_date,
            "odno":"001","ord_gno_brno":"06010","sll_buy_dvsn_cd":"02","ord_qty":"3",
            "ord_unpr":"10000","ord_tmd":"095000","tot_ccld_qty":filled.to_string(),
            "rmn_qty":remaining.to_string(),"cncl_yn":if cancelled {"Y"} else {"N"},"rjct_qty":"0"})
    }

    struct Fixture {
        state: AppState,
        directory: std::path::PathBuf,
        server: tokio::task::JoinHandle<()>,
        cancels: Arc<Mutex<Vec<Value>>>,
    }

    async fn fixture(partial: bool, lost_cancel_reply: bool, paginate: bool) -> Fixture {
        let now = 20_000 * 86_400 - 9 * 3600 + 10 * 3600;
        let base_record = record(now);
        let phase = Arc::new(AtomicU32::new(0));
        let get_phase = phase.clone();
        let cancel_phase = phase.clone();
        let cancels = Arc::new(Mutex::new(Vec::new()));
        let capture = cancels.clone();
        let app = Router::new()
            .route("/uapi/domestic-stock/v1/trading/inquire-daily-ccld",get(move |headers:axum::http::HeaderMap, Query(params): Query<std::collections::HashMap<String,String>>| {
                let phase = get_phase.load(Ordering::Relaxed);
                let record = base_record.clone();
                async move {
                    assert_eq!(headers["tr_id"],"VTTC0081R");
                    assert_eq!(params["ODNO"],"001");
                    if paginate && params["CTX_AREA_NK100"].is_empty() {
                        return ([("tr_cont","M")],Json(json!({"rt_cd":"0","output1":[],"ctx_area_fk100":"fk","ctx_area_nk100":"next"})));
                    }
                    if paginate {assert_eq!(headers["tr_cont"],"N");}
                    let filled = if partial {if phase==0 {1} else {2}} else {0};
                    ([("tr_cont","D")],Json(json!({"rt_cd":"0","output1":[row(&record,filled,3-filled,phase>0)]})))
                }
            }))
            .route("/uapi/domestic-stock/v1/trading/order-rvsecncl",post(move |headers:axum::http::HeaderMap, Json(body):Json<Value>| {
                capture.lock().unwrap().push(body.clone());
                cancel_phase.store(1,Ordering::Relaxed);
                async move {
                    assert_eq!(headers["tr_id"],"VTTC0013U");
                    assert_eq!(body["ORGN_ODNO"],"001");
                    assert_eq!(body["RVSE_CNCL_DVSN_CD"],"02");
                    let code = if lost_cancel_reply {axum::http::StatusCode::BAD_GATEWAY} else {axum::http::StatusCode::OK};
                    (code,Json(json!({"rt_cd":"0","output":{"ODNO":"cancel001"}})))
                }
            }))
            .route("/uapi/domestic-stock/v1/quotations/inquire-price",get(|| async {Json(json!({"rt_cd":"0","output":{"stck_prpr":"9800"}}))}))
            .route("/strategy/trade-plan",post(|| async {
                let mut plan = record(20_000*86_400).plan;
                plan.entry_price = 9_500;
                Json(json!({"model":"mock","plan":plan,"usage":null}))
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let directory = std::env::temp_dir().join(format!(
            "broker-lifecycle-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let mut config = crate::config::AppConfig::load();
        config.news_db_path = directory.join("news.db").to_string_lossy().into_owned();
        config.order_log_path = directory
            .join("orders.jsonl")
            .to_string_lossy()
            .into_owned();
        config.kis_base_url = format!("http://{address}");
        config.strategy_url = format!("http://{address}");
        config.trading_mode = "paper".into();
        config.kis_app_key = "test".into();
        config.kis_app_secret = "test".into();
        config.ai_order_review_seconds = 300;
        config.ai_plan_daily_limit = 40;
        let state = AppState::new(config);
        crate::news::initialize(&state).unwrap();
        ai_plans::initialize(&state).unwrap();
        *state.kis_token.write().await = Some(crate::state::CachedToken {
            access_token: "mock".into(),
            expires_at: None,
            fetched_at: std::time::Instant::now(),
        });
        Fixture {
            state,
            directory,
            server,
            cancels,
        }
    }

    async fn cleanup(fixture: Fixture) {
        fixture.server.abort();
        let _ = fixture.server.await;
        std::fs::remove_dir_all(fixture.directory).unwrap();
    }

    fn risk(state: &AppState) -> RiskSettings {
        let mut risk = RiskSettings::from_config(&state.config);
        risk.auto_trading_budget_krw = 500_000;
        risk.max_order_amount_krw = 100_000;
        risk.max_position_ratio = 0.2;
        risk
    }

    #[tokio::test]
    async fn cancellation_race_preserves_fills_and_waits_for_balance() {
        let fixture = fixture(true, false, true).await;
        let now = 20_000 * 86_400 - 9 * 3600 + 10 * 3600;
        let mut record = record(now);
        record.expires_at_unix = now - 1;
        let risk = risk(&fixture.state);
        manage(
            &fixture.state,
            &mut record,
            (1, 10_000),
            true,
            &risk,
            400_000,
            now,
        )
        .await
        .unwrap();
        assert_eq!(record.order_status, "cancel_pending");
        assert!(record.submitted_side.is_some());
        assert_eq!(fixture.cancels.lock().unwrap()[0]["ORD_QTY"], "2");
        let restarted = AppState::new((*fixture.state.config).clone());
        let mut record = ai_plans::load(&restarted, "005930").unwrap().unwrap();
        *restarted.kis_token.write().await = fixture.state.kis_token.read().await.clone();
        manage(
            &restarted,
            &mut record,
            (1, 10_000),
            true,
            &risk,
            400_000,
            now + 10,
        )
        .await
        .unwrap();
        assert_eq!(record.order_status, "filled_waiting_balance");
        assert!(record.submitted_side.is_some());
        manage(
            &restarted,
            &mut record,
            (2, 20_000),
            true,
            &risk,
            400_000,
            now + 20,
        )
        .await
        .unwrap();
        assert_eq!(record.order_status, "cancelled");
        assert!(record.submitted_side.is_none());
        assert_eq!(record.broker_order.as_ref().unwrap().filled_quantity, 2);
        let mut amounts = std::collections::HashMap::new();
        update_buy_commitments(&restarted, "005930", now - 3600, &mut amounts).unwrap();
        assert_eq!(amounts.values().sum::<u64>(), 20_000);
        assert_eq!(fixture.cancels.lock().unwrap().len(), 1);
        cleanup(fixture).await;
    }

    #[tokio::test]
    async fn ai_reprice_waits_for_confirmed_cancel_even_if_response_is_lost() {
        let fixture = fixture(false, true, false).await;
        let now = 20_000 * 86_400 - 9 * 3600 + 10 * 3600;
        let mut record = record(now);
        let risk = risk(&fixture.state);
        manage(
            &fixture.state,
            &mut record,
            (0, 0),
            true,
            &risk,
            400_000,
            now,
        )
        .await
        .unwrap();
        assert_eq!(record.plan.entry_price, 10_000);
        assert_eq!(record.order_status, "cancel_pending");
        assert_eq!(
            record
                .broker_order
                .as_ref()
                .unwrap()
                .replacement
                .as_ref()
                .unwrap()
                .entry_price,
            9_500
        );
        assert!(record.submitted_side.is_some());
        manage(
            &fixture.state,
            &mut record,
            (0, 0),
            true,
            &risk,
            400_000,
            now + 10,
        )
        .await
        .unwrap();
        assert_eq!(record.order_status, "replacement_ready");
        assert_eq!(record.plan.entry_price, 9_500);
        assert!(record.submitted_side.is_none());
        assert_eq!(fixture.cancels.lock().unwrap().len(), 1);
        cleanup(fixture).await;
    }

    #[test]
    fn unknown_identity_requires_unique_matching_receipt() {
        let now = 20_000 * 86_400 - 9 * 3600 + 10 * 3600;
        let mut record = record(now);
        record.broker_order.as_mut().unwrap().order_no.clear();
        let row = row(&record, 0, 3, false);
        assert!(match_status(&record, std::slice::from_ref(&row)).is_ok());
        assert!(match_status(&record, &[row.clone(), row]).is_err());
        assert!(match_status(&record, &[]).is_err());
    }

    #[test]
    fn opening_auction_session_is_explicit_and_time_bounded() {
        let day = 20_000 * 86_400 - 9 * 3600;
        let mut plan = record(day).plan;
        assert!(!ai_plans::order_session_allowed(
            &plan,
            day + 8 * 3600 + 45 * 60
        ));
        plan.session = "opening_auction".into();
        assert!(!ai_plans::order_session_allowed(
            &plan,
            day + 8 * 3600 + 29 * 60
        ));
        assert!(ai_plans::order_session_allowed(
            &plan,
            day + 8 * 3600 + 30 * 60
        ));
        assert!(ai_plans::order_session_allowed(&plan, day + 9 * 3600));
        assert!(!ai_plans::order_session_allowed(
            &plan,
            day + 86_400 + 8 * 3600 + 45 * 60
        ));
    }

    #[test]
    fn auction_execution_requires_a_positive_indicative_price() {
        let mut quote = kis::to_kis_response(
            json!({"output":{"stck_prpr":"10000"},"output2":{"antc_cnpr":"0"}}),
        );
        assert_eq!(kis::auction_reference_price(&quote), None);
        quote.output2 = Some(json!([{"antc_cnpr":"9,800"}]));
        assert_eq!(kis::auction_reference_price(&quote), Some(9800));
        quote.output2 = None;
        assert_eq!(kis::auction_reference_price(&quote), None);
    }

    #[tokio::test]
    async fn read_only_reconciliation_does_not_cancel_an_expired_order() {
        let fixture = fixture(false, false, false).await;
        let now = 20_000 * 86_400 - 9 * 3600 + 10 * 3600;
        let mut record = record(now);
        record.expires_at_unix = now - 1;
        manage(
            &fixture.state,
            &mut record,
            (0, 0),
            false,
            &risk(&fixture.state),
            400_000,
            now,
        )
        .await
        .unwrap();
        assert!(fixture.cancels.lock().unwrap().is_empty());
        assert!(record.submitted_side.is_some());
        assert_eq!(record.order_status, "unfilled");
        cleanup(fixture).await;
    }
}
