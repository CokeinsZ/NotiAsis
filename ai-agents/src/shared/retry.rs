use std::future::Future;
use std::time::Duration;

/// Reintentos con espera exponencial para llamadas a servicios externos
/// (rate limits y errores 5xx pasajeros).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Intentos totales, contando el primero (1 = sin reintentos).
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl RetryPolicy {
    pub const fn new(max_attempts: u32, base_delay: Duration, max_delay: Duration) -> Self {
        Self { max_attempts, base_delay, max_delay }
    }

    /// Un solo intento: para los tests y las llamadas que no deben repetirse.
    pub const fn no_retries() -> Self {
        Self::new(1, Duration::ZERO, Duration::ZERO)
    }

    /// Espera antes del reintento número `retry` (1, 2, 3...).
    pub fn delay_before(&self, retry: u32) -> Duration {
        let factor = 2u32.saturating_pow(retry.saturating_sub(1));
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }

    /// Ejecuta `operation` y la repite mientras falle con un error que
    /// `is_retryable` considere pasajero y queden intentos.
    pub async fn run<T, E, F, Fut>(&self, mut operation: F, is_retryable: impl Fn(&E) -> bool) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, E>>,
        E: std::fmt::Display,
    {
        let mut attempt = 1;
        loop {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(error) if attempt < self.max_attempts && is_retryable(&error) => {
                    let delay = self.delay_before(attempt);
                    tracing::warn!(attempt, ?delay, %error, "transient error, retrying");
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::new(3, Duration::from_millis(500), Duration::from_secs(8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn delay_grows_exponentially_up_to_the_cap() {
        let policy = RetryPolicy::new(5, Duration::from_millis(100), Duration::from_millis(350));
        assert_eq!(policy.delay_before(1), Duration::from_millis(100));
        assert_eq!(policy.delay_before(2), Duration::from_millis(200));
        assert_eq!(policy.delay_before(3), Duration::from_millis(350));
    }

    #[tokio::test]
    async fn retries_transient_errors_until_success() {
        let calls = AtomicU32::new(0);
        let policy = RetryPolicy::new(3, Duration::ZERO, Duration::ZERO);
        let result: Result<u32, String> = policy
            .run(
                || async {
                    let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
                    if call < 3 { Err("503".to_string()) } else { Ok(call) }
                },
                |_| true,
            )
            .await;
        assert_eq!(result, Ok(3));
    }

    #[tokio::test]
    async fn does_not_retry_permanent_errors() {
        let calls = AtomicU32::new(0);
        let policy = RetryPolicy::new(5, Duration::ZERO, Duration::ZERO);
        let result: Result<(), String> = policy
            .run(
                || async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Err("400".to_string())
                },
                |error| error != "400",
            )
            .await;
        assert!(result.is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
