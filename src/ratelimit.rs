//! A small, dependency-free, in-process rate limiter.
//!
//! Scope and limitations are documented in `docs/security/threat-model.md`:
//!
//! * The counters live in this process only. A horizontally scaled deployment
//!   must place a shared limiter (for example a reverse proxy or Redis) in
//!   front of the API; the per-process limiter is a defence-in-depth control,
//!   not a replacement for edge rate limiting.
//! * The algorithm is a fixed window. It is intentionally simple so that the
//!   behaviour is predictable and testable.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Tracks request counts per client within a fixed window.
#[derive(Debug)]
pub struct RateLimiter {
    max_requests: u32,
    window: Duration,
    entries: Mutex<HashMap<IpAddr, WindowEntry>>,
}

#[derive(Debug, Clone, Copy)]
struct WindowEntry {
    window_start: Instant,
    count: u32,
}

/// Outcome of a rate-limit check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// The request is allowed.
    Allowed,
    /// The request is rejected. `retry_after_seconds` tells the client when
    /// the window resets.
    Rejected { retry_after_seconds: u64 },
}

impl RateLimiter {
    /// Creates a limiter allowing `max_requests` per `window`.
    pub fn new(max_requests: u32, window: Duration) -> Self {
        Self {
            max_requests,
            window,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// Records a request from `client` and reports whether it is allowed.
    pub fn check(&self, client: IpAddr) -> Decision {
        let now = Instant::now();
        let mut entries = match self.entries.lock() {
            Ok(guard) => guard,
            // A poisoned lock must not take the API down; failing open here
            // keeps availability while the operator investigates.
            Err(poisoned) => poisoned.into_inner(),
        };

        // Drop entries whose window has elapsed so the map cannot grow without
        // bound.
        entries.retain(|_, entry| now.duration_since(entry.window_start) < self.window);

        let entry = entries.entry(client).or_insert(WindowEntry {
            window_start: now,
            count: 0,
        });

        if now.duration_since(entry.window_start) >= self.window {
            entry.window_start = now;
            entry.count = 0;
        }

        entry.count += 1;

        if entry.count > self.max_requests {
            let elapsed = now.duration_since(entry.window_start);
            let remaining = self.window.saturating_sub(elapsed);
            Decision::Rejected {
                retry_after_seconds: remaining.as_secs().max(1),
            }
        } else {
            Decision::Allowed
        }
    }

    /// Removes all tracked clients. Used by tests.
    pub fn reset(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, last))
    }

    #[test]
    fn allows_up_to_the_limit_then_rejects() {
        let limiter = RateLimiter::new(3, Duration::from_secs(60));
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert!(matches!(limiter.check(ip(1)), Decision::Rejected { .. }));
    }

    #[test]
    fn clients_are_tracked_independently() {
        let limiter = RateLimiter::new(1, Duration::from_secs(60));
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert!(matches!(limiter.check(ip(1)), Decision::Rejected { .. }));
        assert_eq!(limiter.check(ip(2)), Decision::Allowed);
    }

    #[test]
    fn window_resets_after_the_configured_duration() {
        let limiter = RateLimiter::new(1, Duration::from_millis(30));
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert!(matches!(limiter.check(ip(1)), Decision::Rejected { .. }));
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
    }
}
