use super::super::types::{TmuxControlError, TmuxNotification};
use flume::{Sender, TrySendError};
use std::collections::VecDeque;

pub(crate) const NOTIFICATION_COALESCER_OUTPUT_BYTES_BOUND: usize = 512 * 1024;

#[derive(Debug)]
pub(crate) struct NotificationCoalescer {
    queued: VecDeque<TmuxNotification>,
    has_refresh_queued: bool,
    has_warning_queued: bool,
    queued_output_bytes: usize,
    max_output_bytes: usize,
}

impl Default for NotificationCoalescer {
    fn default() -> Self {
        Self::with_output_byte_limit(NOTIFICATION_COALESCER_OUTPUT_BYTES_BOUND)
    }
}

impl NotificationCoalescer {
    pub(crate) fn with_output_byte_limit(max_output_bytes: usize) -> Self {
        Self {
            queued: VecDeque::new(),
            has_refresh_queued: false,
            has_warning_queued: false,
            queued_output_bytes: 0,
            max_output_bytes,
        }
    }

    fn queue_refresh_if_missing(&mut self) {
        if !self.has_refresh_queued {
            self.has_refresh_queued = true;
            self.queued.push_back(TmuxNotification::NeedsRefresh);
        }
    }

    fn queue_backpressure_warning_if_missing(&mut self, dropped_bytes: usize) {
        if self.has_warning_queued {
            return;
        }

        self.has_warning_queued = true;
        self.queued.push_back(TmuxNotification::Warning(format!(
            "tmux output backlog exceeded {} bytes; dropped {} stale byte(s) and forced refresh",
            self.max_output_bytes, dropped_bytes
        )));
    }

    fn drop_oldest_output_chunk(&mut self) -> Option<usize> {
        let output_index = self
            .queued
            .iter()
            .position(|notification| matches!(notification, TmuxNotification::Output { .. }))?;
        let removed = self.queued.remove(output_index)?;
        match removed {
            TmuxNotification::Output { bytes, .. } => {
                let dropped = bytes.len();
                self.queued_output_bytes = self.queued_output_bytes.saturating_sub(dropped);
                Some(dropped)
            }
            _ => None,
        }
    }

    fn handle_output_backpressure(&mut self, dropped_bytes: usize) {
        self.queue_refresh_if_missing();
        self.queue_backpressure_warning_if_missing(dropped_bytes);
    }

    fn collapse_for_notification_backpressure(&mut self) {
        // The app-side notification channel overflowed, which means the UI
        // consumer is behind. Drop stale pending notifications and force one
        // refresh so the next successful delivery can resynchronize state.
        self.queued.clear();
        self.has_refresh_queued = false;
        self.has_warning_queued = false;
        self.queued_output_bytes = 0;
        self.queue_refresh_if_missing();
    }

    pub(crate) fn push(
        &mut self,
        notification: TmuxNotification,
    ) -> std::result::Result<(), TmuxControlError> {
        match notification {
            TmuxNotification::NeedsRefresh => {
                self.queue_refresh_if_missing();
            }
            TmuxNotification::Output { pane_id, bytes } => {
                if bytes.is_empty() {
                    return Ok(());
                }

                let mut queued_output_bytes =
                    match self.queued_output_bytes.checked_add(bytes.len()) {
                        Some(value) => value,
                        None => {
                            // The pending byte count overflowed arithmetic bounds. Drop the
                            // newest burst, request a refresh, and keep the runtime alive.
                            self.handle_output_backpressure(bytes.len());
                            return Ok(());
                        }
                    };

                if queued_output_bytes > self.max_output_bytes {
                    let mut bytes_to_free =
                        queued_output_bytes.saturating_sub(self.max_output_bytes);
                    while bytes_to_free > 0 {
                        let Some(dropped) = self.drop_oldest_output_chunk() else {
                            break;
                        };
                        self.handle_output_backpressure(dropped);
                        bytes_to_free = bytes_to_free.saturating_sub(dropped);
                    }

                    queued_output_bytes = match self.queued_output_bytes.checked_add(bytes.len()) {
                        Some(value) => value,
                        None => {
                            self.handle_output_backpressure(bytes.len());
                            return Ok(());
                        }
                    };

                    if queued_output_bytes > self.max_output_bytes {
                        // Single bursts can be larger than the byte cap. Drop the burst
                        // and force a refresh instead of exiting tmux runtime.
                        self.handle_output_backpressure(bytes.len());
                        return Ok(());
                    }
                }

                if let Some(TmuxNotification::Output {
                    pane_id: tail_pane_id,
                    bytes: tail_bytes,
                }) = self.queued.back_mut()
                    && *tail_pane_id == pane_id
                {
                    tail_bytes.extend_from_slice(&bytes);
                    self.queued_output_bytes = queued_output_bytes;
                    return Ok(());
                }

                self.queued
                    .push_back(TmuxNotification::Output { pane_id, bytes });
                self.queued_output_bytes = queued_output_bytes;
            }
            TmuxNotification::Warning(message) => {
                if !self.has_warning_queued {
                    self.has_warning_queued = true;
                    self.queued.push_back(TmuxNotification::Warning(message));
                }
            }
            TmuxNotification::Exit(reason) => {
                // Exit is terminal for the UI client. Drop stale backlog so the
                // consumer sees one deterministic shutdown signal.
                self.queued.clear();
                self.has_refresh_queued = false;
                self.has_warning_queued = false;
                self.queued_output_bytes = 0;
                self.queued.push_back(TmuxNotification::Exit(reason));
            }
        }

        Ok(())
    }

    pub(crate) fn pop_next(&mut self) -> Option<TmuxNotification> {
        let notification = self.queued.pop_front()?;
        match &notification {
            TmuxNotification::NeedsRefresh => {
                self.has_refresh_queued = false;
            }
            TmuxNotification::Output { bytes, .. } => {
                self.queued_output_bytes = self.queued_output_bytes.saturating_sub(bytes.len());
            }
            TmuxNotification::Warning(_) => {
                self.has_warning_queued = false;
            }
            TmuxNotification::Exit(_) => {}
        }
        Some(notification)
    }

    pub(crate) fn drain(&mut self) -> Vec<TmuxNotification> {
        let mut drained = Vec::with_capacity(self.queued.len());
        while let Some(notification) = self.pop_next() {
            drained.push(notification);
        }
        drained
    }
}

fn try_send_notification(
    notifications_tx: &Sender<TmuxNotification>,
    notification: TmuxNotification,
) -> std::result::Result<(), TrySendNotificationError> {
    match notifications_tx.try_send(notification) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(notification)) => Err(TrySendNotificationError::Full(notification)),
        Err(TrySendError::Disconnected(_)) => {
            Err(TrySendNotificationError::Disconnected(TmuxControlError::channel(
                "tmux notification channel is closed",
            )))
        }
    }
}

enum TrySendNotificationError {
    Full(TmuxNotification),
    Disconnected(TmuxControlError),
}

fn send_notification_blocking(
    notifications_tx: &Sender<TmuxNotification>,
    notification: TmuxNotification,
) -> std::result::Result<(), TmuxControlError> {
    notifications_tx
        .send(notification)
        .map_err(|_| TmuxControlError::channel("tmux notification channel is closed"))
}

pub(crate) fn signal_event_wakeup(event_wakeup_tx: Option<&Sender<()>>) {
    let Some(event_wakeup_tx) = event_wakeup_tx else {
        return;
    };

    match event_wakeup_tx.try_send(()) {
        Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
    }
}

pub(crate) fn flush_notification_coalescer(
    coalescer: &mut NotificationCoalescer,
    notifications_tx: &Sender<TmuxNotification>,
    event_wakeup_tx: Option<&Sender<()>>,
) -> std::result::Result<(), TmuxControlError> {
    let mut sent_notifications = false;
    while let Some(notification) = coalescer.pop_next() {
        match try_send_notification(notifications_tx, notification) {
            Ok(()) => {
                sent_notifications = true;
            }
            Err(TrySendNotificationError::Full(notification)) => {
                let recovery_notification = match notification {
                    // Exit is terminal. Preserve it even when the UI queue is
                    // saturated so shutdown cannot be downgraded into refresh.
                    TmuxNotification::Exit(reason) => TmuxNotification::Exit(reason),
                    notification => {
                        coalescer.collapse_for_notification_backpressure();
                        coalescer.pop_next().unwrap_or(notification)
                    }
                };
                signal_event_wakeup(event_wakeup_tx);
                // Once the UI drains one stale queued notification, force the
                // recovery signal through immediately so the worker cannot
                // strand shutdown or refresh state locally.
                send_notification_blocking(notifications_tx, recovery_notification)?;
                sent_notifications = true;
                break;
            }
            Err(TrySendNotificationError::Disconnected(error)) => return Err(error),
        }
    }
    if sent_notifications {
        signal_event_wakeup(event_wakeup_tx);
    }
    Ok(())
}

pub(crate) fn signal_fatal_exit(fatal_exit_tx: &Sender<Option<String>>, reason: Option<String>) {
    match fatal_exit_tx.try_send(reason) {
        Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_coalescer_collapses_redundant_refresh_events() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::NeedsRefresh).expect("refresh");
        c.push(TmuxNotification::NeedsRefresh).expect("refresh");

        let drained = c.drain();
        let refresh_count = drained
            .iter()
            .filter(|notification| matches!(notification, TmuxNotification::NeedsRefresh))
            .count();
        assert_eq!(refresh_count, 1);
    }

    #[test]
    fn notification_coalescer_merges_adjacent_output_bursts_per_pane() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b"hello".to_vec(),
        })
        .expect("output");
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b" world".to_vec(),
        })
        .expect("output");
        c.push(TmuxNotification::Output {
            pane_id: "%2".to_string(),
            bytes: b"!".to_vec(),
        })
        .expect("output");

        let drained = c.drain();
        assert_eq!(
            drained,
            vec![
                TmuxNotification::Output {
                    pane_id: "%1".to_string(),
                    bytes: b"hello world".to_vec(),
                },
                TmuxNotification::Output {
                    pane_id: "%2".to_string(),
                    bytes: b"!".to_vec(),
                }
            ]
        );
    }

    #[test]
    fn notification_coalescer_drops_stale_output_and_emits_warning_on_backpressure() {
        let mut c = NotificationCoalescer::with_output_byte_limit(4);
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b"abcd".to_vec(),
        })
        .expect("output");

        c.push(TmuxNotification::Output {
            pane_id: "%2".to_string(),
            bytes: b"efgh".to_vec(),
        })
        .expect("backpressure handling should keep runtime alive");

        let drained = c.drain();
        assert!(
            drained
                .iter()
                .any(|notification| matches!(notification, TmuxNotification::NeedsRefresh)),
            "backpressure should force a refresh"
        );
        assert!(
            drained
                .iter()
                .any(|notification| matches!(notification, TmuxNotification::Warning(_))),
            "backpressure should emit an explicit warning"
        );
        assert!(
            drained.iter().any(|notification| matches!(
                notification,
                TmuxNotification::Output { pane_id, bytes }
                if pane_id == "%2" && bytes == b"efgh"
            )),
            "newest output burst should survive after stale-drop cutover"
        );
    }

    #[test]
    fn notification_coalescer_oversized_single_burst_emits_one_warning_and_one_refresh() {
        let mut c = NotificationCoalescer::with_output_byte_limit(4);
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b"abcdef".to_vec(),
        })
        .expect("oversized burst should not terminate coalescer");

        let drained = c.drain();
        let refresh_count = drained
            .iter()
            .filter(|notification| matches!(notification, TmuxNotification::NeedsRefresh))
            .count();
        let warning_count = drained
            .iter()
            .filter(|notification| matches!(notification, TmuxNotification::Warning(_)))
            .count();
        let output_count = drained
            .iter()
            .filter(|notification| matches!(notification, TmuxNotification::Output { .. }))
            .count();

        assert_eq!(refresh_count, 1);
        assert_eq!(warning_count, 1);
        assert_eq!(output_count, 0);
    }

    #[test]
    fn notification_flush_signals_wakeup_when_notifications_are_enqueued() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::NeedsRefresh).expect("refresh");

        let (notifications_tx, notifications_rx) = flume::bounded(4);
        let (event_wakeup_tx, event_wakeup_rx) = flume::bounded(1);

        flush_notification_coalescer(&mut c, &notifications_tx, Some(&event_wakeup_tx))
            .expect("flush should succeed");

        assert!(matches!(
            notifications_rx.try_recv(),
            Ok(TmuxNotification::NeedsRefresh)
        ));
        assert!(event_wakeup_rx.try_recv().is_ok());
    }

    #[test]
    fn notification_flush_skips_wakeup_when_no_notifications_are_enqueued() {
        let mut c = NotificationCoalescer::default();
        let (notifications_tx, _notifications_rx) = flume::bounded(4);
        let (event_wakeup_tx, event_wakeup_rx) = flume::bounded(1);

        flush_notification_coalescer(&mut c, &notifications_tx, Some(&event_wakeup_tx))
            .expect("flush should succeed");

        assert!(event_wakeup_rx.try_recv().is_err());
    }

    #[test]
    fn notification_flush_queue_overflow_collapses_to_refresh_instead_of_failing() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b"bench".to_vec(),
        })
        .expect("output");

        let (notifications_tx, notifications_rx) = flume::bounded(1);
        notifications_tx
            .send(TmuxNotification::NeedsRefresh)
            .expect("seed full queue");

        let drain_thread = std::thread::spawn(move || {
            assert!(matches!(
                notifications_rx.recv(),
                Ok(TmuxNotification::NeedsRefresh)
            ));
            assert!(matches!(
                notifications_rx.recv(),
                Ok(TmuxNotification::NeedsRefresh)
            ));
        });

        flush_notification_coalescer(&mut c, &notifications_tx, None)
            .expect("queue overflow should degrade instead of failing");
        drain_thread.join().expect("receiver thread should complete");
        assert!(c.drain().is_empty(), "recovery path should collapse to one refresh");
    }

    #[test]
    fn notification_flush_queue_overflow_preserves_exit_notification() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::Exit(Some("tmux exited".to_string())))
            .expect("exit");

        let (notifications_tx, notifications_rx) = flume::bounded(1);
        notifications_tx
            .send(TmuxNotification::NeedsRefresh)
            .expect("seed full queue");

        let drain_thread = std::thread::spawn(move || {
            assert!(matches!(
                notifications_rx.recv(),
                Ok(TmuxNotification::NeedsRefresh)
            ));
            assert!(matches!(
                notifications_rx.recv(),
                Ok(TmuxNotification::Exit(Some(reason))) if reason == "tmux exited"
            ));
        });

        flush_notification_coalescer(&mut c, &notifications_tx, None)
            .expect("queue overflow should preserve exit");
        drain_thread.join().expect("receiver thread should complete");
        assert!(c.drain().is_empty(), "exit recovery should not leave stale backlog");
    }
}
