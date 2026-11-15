use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, MutexGuard, OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, sleep};
use tracing::warn;

#[derive(Debug)]
pub struct RateLimiter {
    max_transactions: u32,
    window: Duration,
    semaphore: Arc<Semaphore>,
    entries: Mutex<VecDeque<(Instant, u32)>>,
}

#[derive(Debug)]
pub struct RateGuard {
    _permit: OwnedSemaphorePermit,
}

impl RateLimiter {
    /// FIRMS default: 5000 transactions / 10 minutes.
    pub fn firms_default(concurrency: usize) -> Self {
        Self::new(5000, Duration::from_secs(10 * 60), concurrency)
    }

    pub fn new(max_transactions: u32, window: Duration, concurrency: usize) -> Self {
        Self {
            max_transactions,
            window,
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// Reserve `cost` transactions, waiting until the sliding window has room.
    /// Also limits in-flight requests to `concurrency`. The returned guard releases
    /// the concurrency slot on drop; the spent transactions stay spent until they
    /// age out of the window.
    pub async fn acquire(&self, cost: u32) -> RateGuard {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("rate limiter semaphore is never closed");

        let effective_cost = if cost > self.max_transactions {
            warn!(
                requested = cost,
                max = self.max_transactions,
                "transaction cost exceeds window limit; clamping to max"
            );
            self.max_transactions
        } else {
            cost
        };

        loop {
            let wait = {
                let mut entries = self.entries.lock().await;
                let now = Instant::now();
                Self::prune(&mut entries, now, self.window);

                let wait = Self::wait_for_locked(
                    &entries,
                    now,
                    self.max_transactions,
                    self.window,
                    effective_cost,
                );
                if wait.is_zero() {
                    if effective_cost > 0 {
                        entries.push_back((now, effective_cost));
                    }
                    return RateGuard { _permit: permit };
                }
                wait
            };

            sleep(wait).await;
        }
    }

    /// Transactions still available in the current window, for logging and for
    /// refusing an obviously impossible bulk plan before starting it.
    pub fn remaining(&self) -> u32 {
        let mut entries = self.lock_entries_sync();
        Self::prune(&mut entries, Instant::now(), self.window);
        let used: u32 = entries.iter().map(|(_, c)| *c).fold(0, u32::saturating_add);
        self.max_transactions.saturating_sub(used)
    }

    /// How long until at least `cost` transactions are free.
    pub fn wait_for(&self, cost: u32) -> Duration {
        let mut entries = self.lock_entries_sync();
        let now = Instant::now();
        Self::prune(&mut entries, now, self.window);
        Self::wait_for_locked(&entries, now, self.max_transactions, self.window, cost)
    }

    fn lock_entries_sync(&self) -> MutexGuard<'_, VecDeque<(Instant, u32)>> {
        // Avoid tokio::sync::Mutex::blocking_lock because it panics inside an async runtime;
        // the lock is never held across an .await, so contention lasts only nanoseconds.
        loop {
            if let Ok(guard) = self.entries.try_lock() {
                return guard;
            }
            std::thread::yield_now();
        }
    }

    fn prune(entries: &mut VecDeque<(Instant, u32)>, now: Instant, window: Duration) {
        while let Some(&(ts, _)) = entries.front() {
            if now.saturating_duration_since(ts) >= window {
                entries.pop_front();
            } else {
                break;
            }
        }
    }

    fn wait_for_locked(
        entries: &VecDeque<(Instant, u32)>,
        now: Instant,
        max_transactions: u32,
        window: Duration,
        cost: u32,
    ) -> Duration {
        let max = u64::from(max_transactions);
        let needed = u64::from(cost).min(max);
        let used: u64 = entries.iter().map(|(_, c)| u64::from(*c)).sum();
        if used + needed <= max {
            return Duration::ZERO;
        }

        let deficit = used + needed - max;
        let mut freed = 0u64;
        for &(ts, amount) in entries {
            freed += u64::from(amount);
            if freed >= deficit {
                return (ts + window).saturating_duration_since(now);
            }
        }

        Duration::ZERO
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[tokio::test]
    async fn remaining_decreases_after_acquire_and_persists_after_drop() {
        tokio::time::pause();
        let limiter = RateLimiter::new(100, Duration::from_secs(600), 4);

        assert_eq!(limiter.remaining(), 100);
        let guard = limiter.acquire(25).await;
        assert_eq!(limiter.remaining(), 75);
        drop(guard);
        assert_eq!(limiter.remaining(), 75);
    }

    #[tokio::test]
    async fn remaining_recovers_after_window_elapses() {
        tokio::time::pause();
        let limiter = RateLimiter::new(100, Duration::from_secs(600), 4);

        let _guard = limiter.acquire(40).await;
        assert_eq!(limiter.remaining(), 60);

        tokio::time::advance(Duration::from_secs(599)).await;
        assert_eq!(limiter.remaining(), 60);

        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(limiter.remaining(), 100);
    }

    #[tokio::test]
    async fn wait_for_is_zero_when_room_and_tracks_expiry_when_full() {
        tokio::time::pause();
        let limiter = RateLimiter::new(10, Duration::from_secs(60), 2);

        assert_eq!(limiter.wait_for(10), Duration::ZERO);
        let _guard = limiter.acquire(6).await;

        assert_eq!(limiter.wait_for(4), Duration::ZERO);
        assert_eq!(limiter.wait_for(5), Duration::from_secs(60));

        tokio::time::advance(Duration::from_secs(20)).await;
        assert_eq!(limiter.wait_for(5), Duration::from_secs(40));
    }

    #[tokio::test]
    async fn acquire_clamps_oversized_cost_without_deadlocking() {
        tokio::time::pause();
        let limiter = RateLimiter::new(10, Duration::from_secs(60), 2);

        let _guard = limiter.acquire(25).await;
        assert_eq!(limiter.remaining(), 0);
    }

    #[tokio::test]
    async fn acquire_sleeps_until_window_frees_room() {
        tokio::time::pause();
        let limiter = RateLimiter::new(10, Duration::from_secs(60), 2);

        drop(limiter.acquire(8).await);
        let start = Instant::now();
        let _guard = limiter.acquire(5).await;

        assert!(start.elapsed() >= Duration::from_secs(60));
        assert_eq!(limiter.remaining(), 5);
    }

    #[tokio::test]
    async fn concurrency_is_bounded() {
        let limiter = Arc::new(RateLimiter::new(100, Duration::from_secs(60), 2));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..6 {
            let limiter = Arc::clone(&limiter);
            let in_flight = Arc::clone(&in_flight);
            let max_seen = Arc::clone(&max_seen);
            handles.push(tokio::spawn(async move {
                let _guard = limiter.acquire(1).await;
                let current = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                max_seen.fetch_max(current, Ordering::SeqCst);
                sleep(Duration::from_millis(15)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        assert!(max_seen.load(Ordering::SeqCst) <= 2);
        assert_eq!(in_flight.load(Ordering::SeqCst), 0);
        assert_eq!(limiter.remaining(), 94);
    }

    #[tokio::test]
    async fn window_walks_idle_partial_full_then_recovers() {
        // State transition 0-switch on the sliding window: Idle -> Partial -> Full -> Idle.
        tokio::time::pause();
        let limiter = RateLimiter::new(10, Duration::from_secs(60), 4);

        assert_eq!(limiter.remaining(), 10);
        drop(limiter.acquire(4).await);
        assert_eq!(limiter.remaining(), 6);
        drop(limiter.acquire(6).await);
        assert_eq!(limiter.remaining(), 0);
        assert_eq!(limiter.wait_for(1), Duration::from_secs(60));

        tokio::time::advance(Duration::from_secs(60)).await;
        assert_eq!(limiter.remaining(), 10);
        assert_eq!(limiter.wait_for(10), Duration::ZERO);
    }

    #[tokio::test]
    async fn full_window_frees_exactly_at_the_window_boundary() {
        // BVA: window-1ms still full, window exactly free.
        tokio::time::pause();
        let limiter = RateLimiter::new(10, Duration::from_secs(60), 1);
        drop(limiter.acquire(10).await);

        tokio::time::advance(Duration::from_millis(59_999)).await;
        assert_eq!(limiter.remaining(), 0);
        assert_eq!(limiter.wait_for(1), Duration::from_millis(1));
        tokio::time::advance(Duration::from_millis(1)).await;
        assert_eq!(limiter.remaining(), 10);
    }

    #[tokio::test]
    async fn entries_expire_oldest_first() {
        tokio::time::pause();
        let limiter = RateLimiter::new(10, Duration::from_secs(60), 4);
        drop(limiter.acquire(3).await);
        tokio::time::advance(Duration::from_secs(30)).await;
        drop(limiter.acquire(4).await);
        assert_eq!(limiter.remaining(), 3);

        tokio::time::advance(Duration::from_secs(30)).await;
        assert_eq!(limiter.remaining(), 6);
        assert_eq!(limiter.wait_for(8), Duration::from_secs(30));

        tokio::time::advance(Duration::from_secs(30)).await;
        assert_eq!(limiter.remaining(), 10);
    }

    #[tokio::test]
    async fn zero_cost_acquire_spends_nothing_and_never_waits_on_a_full_window() {
        tokio::time::pause();
        let limiter = RateLimiter::new(5, Duration::from_secs(60), 2);
        drop(limiter.acquire(5).await);

        let start = Instant::now();
        let _guard = limiter.acquire(0).await;
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(limiter.remaining(), 0);
        assert_eq!(limiter.wait_for(0), Duration::ZERO);
    }

    #[tokio::test]
    async fn exact_remaining_cost_is_granted_immediately_but_one_more_waits() {
        // BVA around the capacity: cost == remaining passes, remaining + 1 waits.
        tokio::time::pause();
        let limiter = RateLimiter::new(10, Duration::from_secs(60), 2);
        drop(limiter.acquire(7).await);

        let start = Instant::now();
        drop(limiter.acquire(3).await);
        assert_eq!(start.elapsed(), Duration::ZERO);

        drop(limiter.acquire(1).await);
        assert!(start.elapsed() >= Duration::from_secs(60) && start.elapsed() < Duration::from_secs(61));
    }

    #[tokio::test]
    async fn zero_concurrency_is_coerced_to_one_slot_that_guard_drop_releases() {
        tokio::time::pause();
        let limiter = RateLimiter::new(100, Duration::from_secs(60), 0);

        let held = limiter.acquire(1).await;
        let blocked = tokio::time::timeout(Duration::from_secs(1), limiter.acquire(1)).await;
        assert!(blocked.is_err(), "second acquire must wait for the only slot");

        drop(held);
        let _next = tokio::time::timeout(Duration::from_secs(1), limiter.acquire(1))
            .await
            .expect("slot is free after the guard drops");
    }

    #[tokio::test]
    async fn firms_default_allows_5000_transactions_and_clamps_larger_requests() {
        tokio::time::pause();
        let limiter = RateLimiter::firms_default(2);
        assert_eq!(limiter.remaining(), 5000);

        drop(limiter.acquire(6000).await);
        assert_eq!(limiter.remaining(), 0);
        assert_eq!(limiter.wait_for(1), Duration::from_secs(600));
    }
}
