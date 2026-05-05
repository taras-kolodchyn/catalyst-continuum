use std::{error::Error, fmt, time::Instant};

use anyhow::{Context, Result};
use redis::{Client, Script};
use uuid::Uuid;

use crate::telemetry;

const REDIS_URL_ENV: &str = "CATALYST_REDIS_URL";
const PROMOTION_LOCK_PREFIX: &str = "catalyst:promotion-lock:run:";
const PROMOTION_LOCK_TTL_SECONDS: u64 = 900;

pub(crate) fn with_promotion_run_lock<T>(
    run_id: Uuid,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let _guard = PromotionRunLock::acquire(run_id)?;
    operation()
}

pub(crate) fn is_promotion_lock_conflict(error: &anyhow::Error) -> bool {
    error.downcast_ref::<PromotionLockConflict>().is_some()
}

struct PromotionRunLock {
    state: Option<PromotionRunLockState>,
}

struct PromotionRunLockState {
    client: Client,
    key: String,
    token: String,
    run_id: Uuid,
}

#[derive(Debug)]
struct PromotionLockConflict {
    run_id: Uuid,
}

impl PromotionRunLock {
    fn acquire(run_id: Uuid) -> Result<Self> {
        let started_at = Instant::now();
        let Some(redis_url) = redis_url_from_env() else {
            telemetry::record_promotion_step("promotion_lock", "disabled", started_at.elapsed());
            tracing::debug!(
                run_id = %run_id,
                "promotion lock disabled because CATALYST_REDIS_URL is not configured"
            );
            return Ok(Self { state: None });
        };

        let client = Client::open(redis_url.as_str())
            .context("failed to configure Redis client for promotion coordination")?;
        let key = promotion_lock_key(run_id);
        let token = Uuid::new_v4().to_string();
        let mut connection = match client.get_connection() {
            Ok(connection) => connection,
            Err(error) => {
                telemetry::record_promotion_step("promotion_lock", "error", started_at.elapsed());
                return Err(error).context("failed to connect to Redis for promotion coordination");
            }
        };

        let acquired: Option<String> = match redis::cmd("SET")
            .arg(&key)
            .arg(&token)
            .arg("NX")
            .arg("EX")
            .arg(PROMOTION_LOCK_TTL_SECONDS)
            .query(&mut connection)
        {
            Ok(acquired) => acquired,
            Err(error) => {
                telemetry::record_promotion_step("promotion_lock", "error", started_at.elapsed());
                return Err(error).context("failed to acquire Redis promotion coordination lock");
            }
        };

        if acquired.as_deref() != Some("OK") {
            telemetry::record_promotion_step("promotion_lock", "contention", started_at.elapsed());
            tracing::warn!(
                run_id = %run_id,
                lock_key = %key,
                "promotion lock is already held for this run"
            );
            return Err(PromotionLockConflict { run_id }.into());
        }

        telemetry::record_promotion_step("promotion_lock", "acquired", started_at.elapsed());
        tracing::info!(
            run_id = %run_id,
            lock_key = %key,
            ttl_seconds = PROMOTION_LOCK_TTL_SECONDS,
            "acquired Redis promotion lock"
        );

        Ok(Self {
            state: Some(PromotionRunLockState {
                client,
                key,
                token,
                run_id,
            }),
        })
    }
}

impl Drop for PromotionRunLock {
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };

        let mut connection = match state.client.get_connection() {
            Ok(connection) => connection,
            Err(error) => {
                tracing::warn!(
                    run_id = %state.run_id,
                    lock_key = %state.key,
                    %error,
                    "failed to reconnect to Redis to release promotion lock"
                );
                return;
            }
        };

        let release_script = Script::new(
            "if redis.call('get', KEYS[1]) == ARGV[1] then \
                 return redis.call('del', KEYS[1]) \
             else \
                 return 0 \
             end",
        );

        match release_script
            .key(state.key.as_str())
            .arg(state.token.as_str())
            .invoke::<u64>(&mut connection)
        {
            Ok(1) => tracing::debug!(
                run_id = %state.run_id,
                lock_key = %state.key,
                "released Redis promotion lock"
            ),
            Ok(_) => tracing::debug!(
                run_id = %state.run_id,
                lock_key = %state.key,
                "promotion lock was already expired or replaced before release"
            ),
            Err(error) => tracing::warn!(
                run_id = %state.run_id,
                lock_key = %state.key,
                %error,
                "failed to release Redis promotion lock"
            ),
        }
    }
}

impl fmt::Display for PromotionLockConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "promotion flow is already in progress for run {}",
            self.run_id
        )
    }
}

impl Error for PromotionLockConflict {}

fn redis_url_from_env() -> Option<String> {
    std::env::var(REDIS_URL_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn promotion_lock_key(run_id: Uuid) -> String {
    format!("{PROMOTION_LOCK_PREFIX}{run_id}")
}

#[cfg(test)]
mod tests {
    use super::{PromotionLockConflict, is_promotion_lock_conflict, promotion_lock_key};

    #[test]
    fn promotion_lock_key_is_run_scoped() {
        let run_id = uuid::Uuid::nil();
        assert_eq!(
            promotion_lock_key(run_id),
            "catalyst:promotion-lock:run:00000000-0000-0000-0000-000000000000"
        );
    }

    #[test]
    fn detects_promotion_lock_conflict_errors() {
        let error = anyhow::Error::new(PromotionLockConflict {
            run_id: uuid::Uuid::nil(),
        });

        assert!(is_promotion_lock_conflict(&error));
    }
}
