use std::time::{Duration, Instant};

const DEFAULT_MAX_ATTEMPTS: usize = 6;
const DEFAULT_RETRY_DELAY_MS: u64 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResizeRequest {
    cols: u16,
    rows: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveResizeConvergence {
    request: ResizeRequest,
    attempts_used: usize,
    next_attempt_at: Instant,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TmuxResizeAttempt {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TmuxResizeWakeup {
    None,
    Immediate,
    Delayed(Duration),
}

#[derive(Clone, Debug)]
pub(crate) struct TmuxResizeScheduler {
    pending: Option<ResizeRequest>,
    active: Option<ActiveResizeConvergence>,
    max_attempts: usize,
    retry_delay: Duration,
}

impl Default for TmuxResizeScheduler {
    fn default() -> Self {
        Self {
            pending: None,
            active: None,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            retry_delay: Duration::from_millis(DEFAULT_RETRY_DELAY_MS),
        }
    }
}

impl TmuxResizeScheduler {
    fn normalize_resize_request(cols: u16, rows: u16) -> ResizeRequest {
        ResizeRequest {
            cols: cols.max(1),
            rows: rows.max(1),
        }
    }

    fn promote_pending_to_active(&mut self, now: Instant) {
        let Some(request) = self.pending.take() else {
            return;
        };
        self.active = Some(ActiveResizeConvergence {
            request,
            attempts_used: 0,
            next_attempt_at: now,
        });
    }

    pub(crate) fn request_resize(&mut self, cols: u16, rows: u16) {
        let request = Self::normalize_resize_request(cols, rows);
        if self.active.is_some_and(|active| active.request == request) {
            self.pending = None;
            return;
        }

        // Hard cutover to the latest viewport size. Older convergence work is stale
        // and can only apply an obsolete layout snapshot.
        self.pending = Some(request);
        self.active = None;
    }

    pub(crate) fn claim_attempt(&mut self, now: Instant) -> Option<TmuxResizeAttempt> {
        if self.active.is_none() {
            self.promote_pending_to_active(now);
        }

        let active = self.active.as_ref()?;
        if active.next_attempt_at > now {
            return None;
        }

        Some(TmuxResizeAttempt {
            cols: active.request.cols,
            rows: active.request.rows,
        })
    }

    pub(crate) fn complete_attempt(&mut self, now: Instant, converged: bool) -> TmuxResizeWakeup {
        let Some(mut active) = self.active.take() else {
            return self.next_wakeup(now);
        };

        if converged {
            self.promote_pending_to_active(now);
            return self.next_wakeup(now);
        }

        active.attempts_used = active.attempts_used.saturating_add(1);
        if active.attempts_used >= self.max_attempts {
            self.promote_pending_to_active(now);
            return self.next_wakeup(now);
        }

        active.next_attempt_at = now + self.retry_delay;
        self.active = Some(active);
        self.next_wakeup(now)
    }

    pub(crate) fn next_wakeup(&self, now: Instant) -> TmuxResizeWakeup {
        if let Some(active) = self.active.as_ref() {
            if active.next_attempt_at <= now {
                return TmuxResizeWakeup::Immediate;
            }
            return TmuxResizeWakeup::Delayed(active.next_attempt_at.duration_since(now));
        }

        if self.pending.is_some() {
            return TmuxResizeWakeup::Immediate;
        }

        TmuxResizeWakeup::None
    }

    pub(crate) fn has_work(&self) -> bool {
        self.pending.is_some() || self.active.is_some()
    }

    pub(crate) fn clear(&mut self) {
        self.pending = None;
        self.active = None;
    }

    #[cfg(test)]
    pub(crate) fn take_pending(&mut self) -> Option<(u16, u16)> {
        self.pending
            .take()
            .map(|pending| (pending.cols, pending.rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_scheduler_coalesces_multiple_resize_requests() {
        let mut scheduler = TmuxResizeScheduler::default();
        scheduler.request_resize(120, 40);
        scheduler.request_resize(121, 40);
        assert_eq!(scheduler.take_pending(), Some((121, 40)));
    }

    #[test]
    fn resize_scheduler_hard_cutover_replaces_in_flight_target() {
        let mut scheduler = TmuxResizeScheduler::default();
        let now = Instant::now();
        scheduler.request_resize(120, 40);
        assert_eq!(
            scheduler.claim_attempt(now),
            Some(TmuxResizeAttempt {
                cols: 120,
                rows: 40
            })
        );

        scheduler.request_resize(130, 50);
        assert_eq!(
            scheduler.claim_attempt(now),
            Some(TmuxResizeAttempt {
                cols: 130,
                rows: 50
            })
        );
    }

    #[test]
    fn resize_scheduler_stops_after_retry_budget_is_exhausted() {
        let mut scheduler = TmuxResizeScheduler::default();
        let mut now = Instant::now();
        scheduler.request_resize(100, 30);

        for _ in 0..DEFAULT_MAX_ATTEMPTS {
            assert!(scheduler.claim_attempt(now).is_some());
            let wakeup = scheduler.complete_attempt(now, false);
            now += Duration::from_millis(DEFAULT_RETRY_DELAY_MS);
            if matches!(wakeup, TmuxResizeWakeup::None) {
                break;
            }
        }

        assert!(!scheduler.has_work());
        assert!(scheduler.claim_attempt(now).is_none());
        assert_eq!(scheduler.next_wakeup(now), TmuxResizeWakeup::None);
    }

    #[test]
    fn resize_scheduler_reports_delayed_wakeup_between_attempts() {
        let mut scheduler = TmuxResizeScheduler::default();
        let now = Instant::now();
        scheduler.request_resize(90, 24);
        assert!(scheduler.claim_attempt(now).is_some());
        let wakeup = scheduler.complete_attempt(now, false);
        assert!(matches!(wakeup, TmuxResizeWakeup::Delayed(delay) if delay > Duration::ZERO));
    }
}
