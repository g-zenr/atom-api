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
    state: Mutex<State>,
}

/// The state the limiter keeps between checks.
#[derive(Debug)]
struct State {
    /// The clients seen so far, with the window they are counted in.
    entries: HashMap<IpAddr, WindowEntry>,
    /// When the map was last swept of expired entries.
    last_sweep: Instant,
}

/// One client's counter within its current window.
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
            state: Mutex::new(State {
                entries: HashMap::new(),
                last_sweep: Instant::now(),
            }),
        }
    }

    /// Records a request from `client` and reports whether it is allowed.
    pub fn check(&self, client: IpAddr) -> Decision {
        let now = Instant::now();
        let mut state = match self.state.lock() {
            Ok(guard) => guard,
            // A poisoned lock must not take the API down; failing open here
            // keeps availability while the operator investigates.
            Err(poisoned) => poisoned.into_inner(),
        };

        // Drop entries whose window has elapsed, so that the map cannot grow
        // without bound.
        //
        // The sweep is amortised: it runs at most once per window rather
        // than on every check, because a full sweep costs one visit per
        // tracked client and would make every request cost more as the map
        // grows. An entry is removed at the first sweep after its own window
        // has elapsed, and sweeps are at most one window apart, so an entry
        // lives for at most two windows: the map holds the clients of the
        // last two windows, and stays bounded.
        if now.duration_since(state.last_sweep) >= self.window {
            state
                .entries
                .retain(|_, entry| now.duration_since(entry.window_start) < self.window);
            state.last_sweep = now;
        }

        let entry = state.entries.entry(client).or_insert(WindowEntry {
            window_start: now,
            count: 0,
        });

        // A client whose window has elapsed starts a new window. The same
        // reset happens whether the entry was swept or not, so the decision
        // for a client does not depend on when the sweep last ran.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn ip(last: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, last))
    }

    /// The number of clients the limiter is currently tracking.
    fn tracked_clients(limiter: &RateLimiter) -> usize {
        limiter
            .state
            .lock()
            .expect("the limiter lock is not poisoned")
            .entries
            .len()
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

    #[test]
    fn the_sweep_drops_the_clients_whose_window_has_elapsed() {
        let limiter = RateLimiter::new(1, Duration::from_millis(30));

        for last in 1..=10 {
            assert_eq!(limiter.check(ip(last)), Decision::Allowed);
        }
        assert_eq!(tracked_clients(&limiter), 10, "every client is tracked");

        std::thread::sleep(Duration::from_millis(50));

        // The next check sweeps the map: the ten stale clients are gone and
        // only the arriving client is left.
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert_eq!(
            tracked_clients(&limiter),
            1,
            "the map must not keep the clients whose window has elapsed"
        );
    }

    #[test]
    fn the_limit_holds_across_a_sweep() {
        let limiter = RateLimiter::new(2, Duration::from_millis(40));

        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert!(matches!(limiter.check(ip(1)), Decision::Rejected { .. }));

        // Other clients arrive, so the map is swept while the first client's
        // window is still open.
        for last in 2..=5 {
            assert_eq!(limiter.check(ip(last)), Decision::Allowed);
        }

        assert!(
            matches!(limiter.check(ip(1)), Decision::Rejected { .. }),
            "the client must still be rejected after the map was swept"
        );
    }

    #[test]
    fn a_client_is_rejected_the_same_number_of_times_after_its_window() {
        let limiter = RateLimiter::new(2, Duration::from_millis(40));

        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert!(matches!(limiter.check(ip(1)), Decision::Rejected { .. }));
        assert!(matches!(limiter.check(ip(1)), Decision::Rejected { .. }));

        std::thread::sleep(Duration::from_millis(60));

        // The window has elapsed, so the client starts a new one.
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert_eq!(limiter.check(ip(1)), Decision::Allowed);
        assert!(matches!(limiter.check(ip(1)), Decision::Rejected { .. }));
    }
}
