use crate::{
    error::{api_error, ApiResult},
    kis::KisApiResponse,
};
use axum::http::StatusCode;
use std::{collections::HashMap, future::Future, sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, Semaphore},
    time::{timeout, Instant},
};

type Slot = Arc<Mutex<Option<Entry>>>;
struct Entry {
    expires: Instant,
    result: ApiResult<KisApiResponse>,
}

pub struct DashboardCache {
    entries: Mutex<HashMap<String, Slot>>,
    requests: Semaphore,
}

impl Default for DashboardCache {
    fn default() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            requests: Semaphore::new(2),
        }
    }
}

impl DashboardCache {
    // UI-only: order decisions and broker reconciliation must continue using fresh data.
    pub async fn get<F, Fut>(&self, key: String, fetch: F) -> ApiResult<KisApiResponse>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ApiResult<KisApiResponse>>,
    {
        timeout(Duration::from_secs(8), async {
            let slot = {
                let mut entries = self.entries.lock().await;
                if entries.len() >= 128 {
                    entries.retain(|_, slot| Arc::strong_count(slot) > 1);
                }
                if entries.len() >= 128 && !entries.contains_key(&key) {
                    return Err(api_error(
                        StatusCode::TOO_MANY_REQUESTS,
                        "dashboard_busy",
                        "화면 조회가 혼잡합니다. 잠시 후 다시 시도해 주세요.",
                    ));
                }
                entries
                    .entry(key)
                    .or_insert_with(|| Arc::new(Mutex::new(None)))
                    .clone()
            };
            let mut entry = slot.lock().await;
            if let Some(cached) = entry
                .as_ref()
                .filter(|entry| entry.expires > Instant::now())
            {
                return cached.result.clone();
            }
            let _permit = self.requests.try_acquire().map_err(|_| {
                api_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "dashboard_busy",
                    "시세 조회가 혼잡합니다. 잠시 후 새로고침해 주세요.",
                )
            })?;
            let result = fetch().await;
            let ttl = if result.is_ok() { 5 } else { 10 };
            *entry = Some(Entry {
                expires: Instant::now() + Duration::from_secs(ttl),
                result: result.clone(),
            });
            result
        })
        .await
        .unwrap_or_else(|_| {
            Err(api_error(
                StatusCode::GATEWAY_TIMEOUT,
                "dashboard_timeout",
                "증권사 조회 대기시간을 초과했습니다. 잠시 후 새로고침해 주세요.",
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[tokio::test]
    async fn shares_inflight_requests_and_temporarily_caches_failures() {
        let cache = DashboardCache::default();
        let calls = AtomicUsize::new(0);
        let fetch = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            Err(api_error(
                StatusCode::TOO_MANY_REQUESTS,
                "kis_rate_limited",
                "limit",
            ))
        };
        let (a, b) = tokio::join!(
            cache.get("balance".into(), fetch),
            cache.get("balance".into(), fetch)
        );
        assert!(a.is_err() && b.is_err());
        assert!(cache.get("balance".into(), fetch).await.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fresh_success_is_shared_but_expired_values_are_refetched() {
        let cache = DashboardCache::default();
        let calls = AtomicUsize::new(0);
        let fetch = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(crate::kis::to_kis_response(
                serde_json::json!({"rt_cd":"0","output":{"price":"100"}}),
            ))
        };
        assert!(cache.get("quote".into(), fetch).await.is_ok());
        assert!(cache.get("quote".into(), fetch).await.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let slot = cache.entries.lock().await["quote"].clone();
        slot.lock().await.as_mut().unwrap().expires = Instant::now();
        assert!(cache.get("quote".into(), fetch).await.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn overload_returns_without_adding_another_broker_request() {
        let cache = DashboardCache::default();
        let _permits = cache.requests.acquire_many(2).await.unwrap();
        let result = cache
            .get("balance".into(), || async {
                panic!("Must not call broker")
            })
            .await;
        assert!(matches!(result, Err((StatusCode::TOO_MANY_REQUESTS, _))));
    }
}
