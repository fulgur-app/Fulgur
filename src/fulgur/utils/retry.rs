//! Retry utilities with exponential backoff for network operations
//!
//! Provides reusable retry logic to handle transient network failures gracefully.

use std::thread;
use std::time::Duration;

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (0 = no retries, just one attempt)
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries (cap for exponential backoff)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (typically 2.0)
    pub backoff_multiplier: f64,
}

impl Default for RetryConfig {
    /// Creates a default RetryConfig
    ///
    /// ### Returns
    /// `Self`: a default configuration
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

impl RetryConfig {
    /// Create a config for aggressive retries (useful for critical operations)
    ///
    /// ### Returns
    /// `Self`: an aggressive configuration
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
        }
    }

    /// Create a config for conservative retries (useful for background operations)
    ///
    /// ### Returns
    /// `Self`: a conservative configuration
    pub fn conservative() -> Self {
        Self {
            max_attempts: 2,
            initial_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
        }
    }
}

/// Execute a function with retry logic and exponential backoff
///
/// ### Arguments
/// - `config`: Retry configuration
/// - `operation`: Function to retry (should be idempotent)
///
/// ### Returns
/// - `Ok(T)`: Result from successful operation
/// - `Err(E)`: Error from last failed attempt
pub fn with_retry<T, E, F>(config: RetryConfig, mut operation: F) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut attempt = 0;
    let mut delay = config.initial_delay;

    loop {
        attempt += 1;

        match operation() {
            Ok(result) => {
                if attempt > 1 {
                    log::info!("Operation succeeded after {} attempts", attempt);
                }
                return Ok(result);
            }
            Err(error) => {
                if attempt >= config.max_attempts {
                    log::error!("Operation failed after {} attempts, giving up", attempt);
                    return Err(error);
                }

                log::warn!(
                    "Operation failed (attempt {}/{}), retrying after {:?}",
                    attempt,
                    config.max_attempts,
                    delay
                );

                thread::sleep(delay);

                // Exponential backoff with cap
                delay = Duration::from_secs_f64(
                    (delay.as_secs_f64() * config.backoff_multiplier)
                        .min(config.max_delay.as_secs_f64()),
                );
            }
        }
    }
}

/// Exponential backoff calculator for connection loops (SSE, WebSocket, etc.)
///
/// Tracks consecutive failures and calculates appropriate delay with exponential backoff.
/// Resets on success.
pub struct BackoffCalculator {
    consecutive_failures: u32,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
}

impl BackoffCalculator {
    /// Create a new backoff calculator
    ///
    /// ### Arguments
    /// - `initial_delay`: Starting delay (e.g., 1 second)
    /// - `max_delay`: Maximum delay cap (e.g., 5 minutes)
    /// - `multiplier`: Backoff multiplier (typically 2.0)
    ///
    /// ### Returns
    /// `Self`: a new BackoffCalculator
    pub fn new(initial_delay: Duration, max_delay: Duration, multiplier: f64) -> Self {
        Self {
            consecutive_failures: 0,
            initial_delay,
            max_delay,
            multiplier,
        }
    }

    /// Create with default settings (1s initial, 5min max, 2x multiplier)
    ///
    /// ### Returns
    /// `Self`: A BackoffCalculator with default settings
    pub fn default_settings() -> Self {
        Self::new(
            Duration::from_secs(1),
            Duration::from_secs(300), // 5 minutes
            2.0,
        )
    }

    /// Record a failure and return the delay to wait before next attempt
    ///
    /// ### Returns
    /// `Duration`: the duration to wait before the next attempt
    pub fn record_failure(&mut self) -> Duration {
        self.consecutive_failures += 1;

        let exponent = (self.consecutive_failures - 1) as u32;
        let delay_secs = self.initial_delay.as_secs_f64() * self.multiplier.powi(exponent as i32);
        let capped_delay = delay_secs.min(self.max_delay.as_secs_f64());

        Duration::from_secs_f64(capped_delay)
    }

    /// Record a success (resets consecutive failures)
    pub fn record_success(&mut self) {
        if self.consecutive_failures > 0 {
            log::info!(
                "Connection recovered after {} consecutive failures",
                self.consecutive_failures
            );
            self.consecutive_failures = 0;
        }
    }

    /// Get the number of consecutive failures
    ///
    /// ### Returns
    /// ù32`: the number of consecutive failures
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_success_first_attempt() {
        let config = RetryConfig::default();
        let mut attempts = 0;

        let result = with_retry(config, || {
            attempts += 1;
            Ok::<i32, String>(42)
        });

        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 1);
    }

    #[test]
    fn test_retry_success_after_failures() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };
        let mut attempts = 0;

        let result = with_retry(config, || {
            attempts += 1;
            if attempts < 3 {
                Err("temporary failure")
            } else {
                Ok::<i32, &str>(42)
            }
        });

        assert_eq!(result, Ok(42));
        assert_eq!(attempts, 3);
    }

    #[test]
    fn test_retry_exhausted() {
        let config = RetryConfig {
            max_attempts: 2,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
        };
        let mut attempts = 0;

        let result = with_retry(config, || {
            attempts += 1;
            Err::<i32, &str>("permanent failure")
        });

        assert_eq!(result, Err("permanent failure"));
        assert_eq!(attempts, 2);
    }

    #[test]
    fn test_backoff_calculator_progression() {
        let mut backoff =
            BackoffCalculator::new(Duration::from_secs(1), Duration::from_secs(60), 2.0);

        // First failure: 1s
        let delay1 = backoff.record_failure();
        assert_eq!(delay1, Duration::from_secs(1));

        // Second failure: 2s
        let delay2 = backoff.record_failure();
        assert_eq!(delay2, Duration::from_secs(2));

        // Third failure: 4s
        let delay3 = backoff.record_failure();
        assert_eq!(delay3, Duration::from_secs(4));

        // Fourth failure: 8s
        let delay4 = backoff.record_failure();
        assert_eq!(delay4, Duration::from_secs(8));
    }

    #[test]
    fn test_backoff_calculator_max_cap() {
        let mut backoff =
            BackoffCalculator::new(Duration::from_secs(1), Duration::from_secs(10), 2.0);

        // Keep failing until we hit the cap
        for _ in 0..10 {
            backoff.record_failure();
        }

        let delay = backoff.record_failure();
        assert!(delay <= Duration::from_secs(10));
    }

    #[test]
    fn test_backoff_calculator_reset_on_success() {
        let mut backoff = BackoffCalculator::default_settings();

        // Record some failures
        backoff.record_failure();
        backoff.record_failure();
        assert_eq!(backoff.consecutive_failures(), 2);

        // Record success
        backoff.record_success();
        assert_eq!(backoff.consecutive_failures(), 0);

        // Next failure should start from initial delay
        let delay = backoff.record_failure();
        assert_eq!(delay, Duration::from_secs(1));
    }

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_retry_config_aggressive() {
        let config = RetryConfig::aggressive();
        assert_eq!(config.max_attempts, 5);
        assert!(config.initial_delay < Duration::from_secs(1));
    }

    #[test]
    fn test_retry_config_conservative() {
        let config = RetryConfig::conservative();
        assert_eq!(config.max_attempts, 2);
        assert!(config.initial_delay >= Duration::from_secs(2));
    }
}
