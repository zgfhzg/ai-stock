use axum::{extract::State, http::StatusCode, Json};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Deserialize, Serialize)]
pub struct Principal {
    pub principal_krw: Option<u64>,
}

fn database(state: &AppState) -> rusqlite::Result<Connection> {
    let db = Connection::open(&state.config.news_db_path)?;
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS account_principal (
        account_key TEXT PRIMARY KEY, principal_krw INTEGER NOT NULL
    )",
    )?;
    Ok(db)
}

fn key(state: &AppState) -> String {
    format!(
        "{}:{}:{}",
        state.config.trading_mode,
        state.config.kis_account_no,
        state.config.kis_account_product_code
    )
}

fn failure(error: rusqlite::Error) -> (StatusCode, Json<ApiError>) {
    tracing::warn!("account principal storage: {}", error);
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            code: "principal_storage_failed".into(),
            message: "원금 설정을 읽거나 저장하지 못했습니다.".into(),
        }),
    )
}

pub async fn get(State(state): State<AppState>) -> ApiResult<Json<Principal>> {
    let db = database(&state).map_err(failure)?;
    let principal_krw = db
        .query_row(
            "SELECT principal_krw FROM account_principal WHERE account_key=?1",
            [key(&state)],
            |r| r.get(0),
        )
        .optional()
        .map_err(failure)?;
    Ok(Json(Principal { principal_krw }))
}

pub async fn save(
    State(state): State<AppState>,
    Json(input): Json<Principal>,
) -> ApiResult<Json<Principal>> {
    if input
        .principal_krw
        .is_some_and(|v| v == 0 || v > 9_007_199_254_740_991)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                code: "invalid_principal".into(),
                message: "원금은 0보다 큰 정수 금액이어야 합니다.".into(),
            }),
        ));
    }
    let db = database(&state).map_err(failure)?;
    if let Some(amount) = input.principal_krw {
        db.execute(
            "INSERT INTO account_principal(account_key,principal_krw) VALUES (?1,?2)
            ON CONFLICT(account_key) DO UPDATE SET principal_krw=excluded.principal_krw",
            params![key(&state), amount],
        )
        .map_err(failure)?;
    } else {
        db.execute(
            "DELETE FROM account_principal WHERE account_key=?1",
            [key(&state)],
        )
        .map_err(failure)?;
    }
    Ok(Json(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn principal_persists_and_is_isolated_from_other_accounts_and_modes() {
        let path = std::env::temp_dir().join(format!(
            "principal-{}.db",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut config = crate::config::AppConfig::load();
        config.news_db_path = path.to_string_lossy().into_owned();
        config.kis_account_no = "test-account".into();
        config.trading_mode = "paper".into();
        let state = AppState::new(config.clone());
        assert_eq!(
            get(State(state.clone()))
                .await
                .ok()
                .unwrap()
                .0
                .principal_krw,
            None
        );
        for amount in [0, 9_007_199_254_740_992] {
            assert!(save(
                State(state.clone()),
                Json(Principal {
                    principal_krw: Some(amount)
                })
            )
            .await
            .is_err());
        }
        let _ = save(
            State(state),
            Json(Principal {
                principal_krw: Some(10_000_000),
            }),
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(
            get(State(AppState::new(config.clone())))
                .await
                .ok()
                .unwrap()
                .0
                .principal_krw,
            Some(10_000_000)
        );
        let mut other = config.clone();
        other.trading_mode = "live".into();
        assert_eq!(
            get(State(AppState::new(other)))
                .await
                .ok()
                .unwrap()
                .0
                .principal_krw,
            None
        );
        let mut other = config.clone();
        other.kis_account_no = "other-account".into();
        assert_eq!(
            get(State(AppState::new(other)))
                .await
                .ok()
                .unwrap()
                .0
                .principal_krw,
            None
        );
        let _ = save(
            State(AppState::new(config.clone())),
            Json(Principal {
                principal_krw: None,
            }),
        )
        .await
        .ok()
        .unwrap();
        assert_eq!(
            get(State(AppState::new(config)))
                .await
                .ok()
                .unwrap()
                .0
                .principal_krw,
            None
        );
        std::fs::remove_file(path).unwrap();
    }
}
