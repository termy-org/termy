use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
    Loading,
}

/// Duration of the fade-in animation in milliseconds
pub const TOAST_FADE_IN_MS: u64 = 150;
/// Duration of the fade-out animation in milliseconds
pub const TOAST_FADE_OUT_MS: u64 = 200;

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub message: String,
    pub created_at: Instant,
    pub paused_at: Option<Instant>,
    pub paused_total: Duration,
    pub duration: Duration,
}

impl Toast {
    fn elapsed(&self) -> Duration {
        let now = Instant::now();
        let total = now.duration_since(self.created_at);
        let active_pause = self
            .paused_at
            .map(|paused_at| now.duration_since(paused_at))
            .unwrap_or_default();
        total.saturating_sub(self.paused_total + active_pause)
    }

    /// Returns animation progress from 0.0 to 1.0 for fade-in/fade-out
    /// 0.0 = fully transparent, 1.0 = fully visible
    pub fn opacity(&self) -> f32 {
        let elapsed = self.elapsed();
        let elapsed_ms = elapsed.as_millis() as u64;

        // Fade in
        if elapsed_ms < TOAST_FADE_IN_MS {
            return elapsed_ms as f32 / TOAST_FADE_IN_MS as f32;
        }

        // Fade out (last FADE_OUT_MS of the duration)
        let remaining = self.duration.saturating_sub(elapsed);
        let remaining_ms = remaining.as_millis() as u64;

        if remaining_ms < TOAST_FADE_OUT_MS {
            return remaining_ms as f32 / TOAST_FADE_OUT_MS as f32;
        }

        1.0
    }

    /// Returns vertical offset for slide-in animation (0.0 = final position)
    pub fn slide_offset(&self) -> f32 {
        let elapsed_ms = self.elapsed().as_millis() as u64;

        if elapsed_ms < TOAST_FADE_IN_MS {
            let progress = elapsed_ms as f32 / TOAST_FADE_IN_MS as f32;
            // Ease out cubic
            let eased = 1.0 - (1.0 - progress).powi(3);
            return 20.0 * (1.0 - eased);
        }

        0.0
    }
}

#[derive(Clone, Debug)]
pub struct ToastRequest {
    pub kind: ToastKind,
    pub message: String,
    pub duration: Duration,
}

#[derive(Default)]
pub struct ToastManager {
    next_id: u64,
    active: Vec<Toast>,
}

impl ToastManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn active(&self) -> &[Toast] {
        &self.active
    }

    pub fn push(&mut self, request: ToastRequest) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.active.push(Toast {
            id,
            kind: request.kind,
            message: request.message,
            created_at: Instant::now(),
            paused_at: None,
            paused_total: Duration::ZERO,
            duration: request.duration,
        });
        id
    }

    pub fn dismiss(&mut self, id: u64) {
        self.active.retain(|toast| toast.id != id);
    }

    /// Tick with optional hovered toast ID - hovered toasts don't expire
    pub fn tick_with_hovered(&mut self, hovered_id: Option<u64>) {
        let now = Instant::now();
        for toast in self.active.iter_mut() {
            let is_hovered = hovered_id == Some(toast.id);
            match (is_hovered, toast.paused_at) {
                (true, None) => {
                    // Refresh lifetime when hovering begins so repeated hover keeps the toast alive.
                    toast.created_at = now - Duration::from_millis(TOAST_FADE_IN_MS);
                    toast.paused_total = Duration::ZERO;
                    toast.paused_at = Some(now);
                }
                (false, Some(paused_at)) => {
                    toast.paused_total += now.duration_since(paused_at);
                    toast.paused_at = None;
                }
                _ => {}
            }
        }

        self.active.retain(|toast| toast.elapsed() < toast.duration);
    }

    pub fn tick(&mut self) {
        self.tick_with_hovered(None);
    }

    /// Pause a toast's timer.
    pub fn pause(&mut self, id: u64) {
        if let Some(toast) = self.active.iter_mut().find(|t| t.id == id)
            && toast.paused_at.is_none()
        {
            toast.paused_at = Some(Instant::now());
        }
    }

    /// Resume a paused toast's timer.
    pub fn resume(&mut self, id: u64) {
        if let Some(toast) = self.active.iter_mut().find(|t| t.id == id)
            && let Some(paused_at) = toast.paused_at.take()
        {
            toast.paused_total += Instant::now().duration_since(paused_at);
        }
    }

    pub fn ingest_pending(&mut self) {
        for request in drain_pending() {
            self.push(request);
        }
        for request in drain_pending_with_id() {
            self.push_with_id(request);
        }
        for update in drain_pending_updates() {
            self.apply_update(update);
        }
        for dismiss in drain_pending_dismisses() {
            self.dismiss(dismiss.id);
        }
    }

    pub fn push_with_id(&mut self, request: ToastRequestWithId) {
        self.active.push(Toast {
            id: request.id,
            kind: request.kind,
            message: request.message,
            created_at: Instant::now(),
            paused_at: None,
            paused_total: Duration::ZERO,
            duration: request.duration,
        });
    }

    pub fn apply_update(&mut self, update: ToastUpdate) {
        if let Some(toast) = self.active.iter_mut().find(|t| t.id == update.id) {
            toast.kind = update.kind;
            toast.message = update.message;
            // Reset duration to default for non-loading toasts
            if update.kind != ToastKind::Loading {
                toast.duration = DEFAULT_TOAST_DURATION;
                // Reset created_at so it starts fresh timing from now
                toast.created_at = Instant::now();
                toast.paused_at = None;
                toast.paused_total = Duration::ZERO;
            }
        }
    }

    /// Returns true if any toast is currently animating (fade in, fade out, or loading spinner)
    pub fn is_animating(&self) -> bool {
        self.active.iter().any(|toast| {
            // Loading toasts are always animating (spinner)
            if toast.kind == ToastKind::Loading {
                return true;
            }

            let elapsed = toast.elapsed();
            let elapsed_ms = elapsed.as_millis() as u64;
            let remaining_ms = toast.duration.saturating_sub(elapsed).as_millis() as u64;

            // Animating if in fade-in or fade-out period
            elapsed_ms < TOAST_FADE_IN_MS || remaining_ms < TOAST_FADE_OUT_MS
        })
    }
}

const DEFAULT_TOAST_DURATION: Duration = Duration::from_millis(3000);

static TOAST_QUEUE: OnceLock<Mutex<Vec<ToastRequest>>> = OnceLock::new();

fn queue() -> &'static Mutex<Vec<ToastRequest>> {
    TOAST_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn enqueue_toast(kind: ToastKind, message: impl Into<String>, duration: Option<Duration>) {
    let request = ToastRequest {
        kind,
        message: message.into(),
        duration: duration.unwrap_or(DEFAULT_TOAST_DURATION),
    };

    let mut queue = queue().lock().expect("toast queue lock poisoned");
    queue.push(request);
}

pub fn drain_pending() -> Vec<ToastRequest> {
    let mut queue = queue().lock().expect("toast queue lock poisoned");
    std::mem::take(&mut *queue)
}

pub fn info(message: impl Into<String>) {
    enqueue_toast(ToastKind::Info, message, None);
}

pub fn success(message: impl Into<String>) {
    enqueue_toast(ToastKind::Success, message, None);
}

pub fn warning(message: impl Into<String>) {
    enqueue_toast(ToastKind::Warning, message, None);
}

pub fn error(message: impl Into<String>) {
    enqueue_toast(ToastKind::Error, message, None);
}

/// Show an info toast that stays longer (6 seconds)
pub fn info_long(message: impl Into<String>) {
    enqueue_toast(ToastKind::Info, message, Some(Duration::from_millis(6000)));
}

/// Show a success toast that stays longer (6 seconds)
pub fn success_long(message: impl Into<String>) {
    enqueue_toast(
        ToastKind::Success,
        message,
        Some(Duration::from_millis(6000)),
    );
}

/// Show an error toast that stays longer (8 seconds)
pub fn error_long(message: impl Into<String>) {
    enqueue_toast(ToastKind::Error, message, Some(Duration::from_millis(8000)));
}

/// Show a loading toast (stays indefinitely until updated or dismissed)
pub fn loading(message: impl Into<String>) -> u64 {
    enqueue_toast_with_id(
        ToastKind::Loading,
        message,
        Some(Duration::from_secs(60 * 60)), // 1 hour (effectively indefinite)
    )
}

/// Enqueue a toast and return its ID for later updates
pub fn enqueue_toast_with_id(
    kind: ToastKind,
    message: impl Into<String>,
    duration: Option<Duration>,
) -> u64 {
    static NEXT_PENDING_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let id = NEXT_PENDING_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let request = ToastRequestWithId {
        id,
        kind,
        message: message.into(),
        duration: duration.unwrap_or(DEFAULT_TOAST_DURATION),
    };

    let mut queue = pending_with_id().lock().expect("toast queue lock poisoned");
    queue.push(request);
    id
}

/// Update an existing toast's kind and message
pub fn update_toast(id: u64, kind: ToastKind, message: impl Into<String>) {
    let update = ToastUpdate {
        id,
        kind,
        message: message.into(),
    };
    let mut queue = pending_updates()
        .lock()
        .expect("toast update queue lock poisoned");
    queue.push(update);
}

#[derive(Clone, Debug)]
pub struct ToastRequestWithId {
    pub id: u64,
    pub kind: ToastKind,
    pub message: String,
    pub duration: Duration,
}

#[derive(Clone, Debug)]
pub struct ToastUpdate {
    pub id: u64,
    pub kind: ToastKind,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct ToastDismiss {
    pub id: u64,
}

static TOAST_QUEUE_WITH_ID: OnceLock<Mutex<Vec<ToastRequestWithId>>> = OnceLock::new();
static TOAST_UPDATE_QUEUE: OnceLock<Mutex<Vec<ToastUpdate>>> = OnceLock::new();
static TOAST_DISMISS_QUEUE: OnceLock<Mutex<Vec<ToastDismiss>>> = OnceLock::new();

fn pending_with_id() -> &'static Mutex<Vec<ToastRequestWithId>> {
    TOAST_QUEUE_WITH_ID.get_or_init(|| Mutex::new(Vec::new()))
}

fn pending_updates() -> &'static Mutex<Vec<ToastUpdate>> {
    TOAST_UPDATE_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

fn pending_dismisses() -> &'static Mutex<Vec<ToastDismiss>> {
    TOAST_DISMISS_QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn drain_pending_with_id() -> Vec<ToastRequestWithId> {
    let mut queue = pending_with_id().lock().expect("toast queue lock poisoned");
    std::mem::take(&mut *queue)
}

pub fn drain_pending_updates() -> Vec<ToastUpdate> {
    let mut queue = pending_updates()
        .lock()
        .expect("toast update queue lock poisoned");
    std::mem::take(&mut *queue)
}

pub fn dismiss_toast(id: u64) {
    let mut queue = pending_dismisses()
        .lock()
        .expect("toast dismiss queue lock poisoned");
    queue.push(ToastDismiss { id });
}

pub fn drain_pending_dismisses() -> Vec<ToastDismiss> {
    let mut queue = pending_dismisses()
        .lock()
        .expect("toast dismiss queue lock poisoned");
    std::mem::take(&mut *queue)
}
