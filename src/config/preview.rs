use std::sync::{LazyLock, Mutex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackgroundOpacityPreview {
    pub owner_id: u64,
    pub opacity: f32,
}

static BACKGROUND_OPACITY_PREVIEW: LazyLock<Mutex<Option<BackgroundOpacityPreview>>> =
    LazyLock::new(|| Mutex::new(None));
static BACKGROUND_OPACITY_PREVIEW_SUBSCRIBERS: LazyLock<
    Mutex<Vec<flume::Sender<Option<BackgroundOpacityPreview>>>>,
> = LazyLock::new(|| Mutex::new(Vec::new()));

pub fn publish_background_opacity_preview(preview: Option<BackgroundOpacityPreview>) {
    if let Ok(mut stored_preview) = BACKGROUND_OPACITY_PREVIEW.lock() {
        *stored_preview = preview.map(|preview| BackgroundOpacityPreview {
            opacity: preview.opacity.clamp(0.0, 1.0),
            ..preview
        });
    }

    let Ok(mut subscribers) = BACKGROUND_OPACITY_PREVIEW_SUBSCRIBERS.lock() else {
        return;
    };
    #[cfg(test)]
    subscribers.retain(test_subscriber_is_alive);

    #[cfg(not(test))]
    {
        let current_preview = current_background_opacity_preview();
        subscribers.retain(|tx| tx.send(current_preview).is_ok());
    }
}

#[cfg(test)]
fn test_subscriber_is_alive(tx: &flume::Sender<Option<BackgroundOpacityPreview>>) -> bool {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    catch_unwind(AssertUnwindSafe(|| {
        tx.send(current_background_opacity_preview())
    }))
    .ok()
    .is_some_and(|result| result.is_ok())
}

pub fn subscribe_background_opacity_preview() -> flume::Receiver<Option<BackgroundOpacityPreview>> {
    let (tx, rx) = flume::unbounded();
    if let Ok(mut subscribers) = BACKGROUND_OPACITY_PREVIEW_SUBSCRIBERS.lock() {
        subscribers.push(tx);
    }
    rx
}

pub fn current_background_opacity_preview() -> Option<BackgroundOpacityPreview> {
    BACKGROUND_OPACITY_PREVIEW
        .lock()
        .ok()
        .and_then(|preview| *preview)
}

pub fn effective_background_opacity(
    saved_opacity: f32,
    preview_opacity: Option<BackgroundOpacityPreview>,
) -> f32 {
    preview_opacity
        .map(|preview| preview.opacity)
        .unwrap_or(saved_opacity)
        .clamp(0.0, 1.0)
}

pub fn synced_background_opacity_preview(
    saved_opacity: f32,
    preview_opacity: Option<BackgroundOpacityPreview>,
) -> Option<BackgroundOpacityPreview> {
    let saved_opacity = saved_opacity.clamp(0.0, 1.0);
    preview_opacity
        .map(|preview| BackgroundOpacityPreview {
            opacity: preview.opacity.clamp(0.0, 1.0),
            ..preview
        })
        .filter(|preview| (preview.opacity - saved_opacity).abs() >= f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn preview(owner_id: u64, opacity: f32) -> BackgroundOpacityPreview {
        BackgroundOpacityPreview { owner_id, opacity }
    }

    #[test]
    fn publishing_preview_notifies_subscribers() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        publish_background_opacity_preview(None);
        let rx = subscribe_background_opacity_preview();

        publish_background_opacity_preview(Some(preview(7, 0.45)));

        assert_eq!(
            rx.recv().expect("preview notification"),
            Some(preview(7, 0.45))
        );
    }

    #[test]
    fn publishing_none_clears_preview() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        publish_background_opacity_preview(Some(preview(7, 0.45)));
        let rx = subscribe_background_opacity_preview();

        publish_background_opacity_preview(None);

        assert_eq!(rx.recv().expect("preview clear notification"), None);
        assert_eq!(current_background_opacity_preview(), None);
    }

    #[test]
    fn current_preview_reflects_latest_value() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        publish_background_opacity_preview(Some(preview(7, 0.2)));
        assert_eq!(current_background_opacity_preview(), Some(preview(7, 0.2)));

        publish_background_opacity_preview(Some(preview(7, 1.5)));
        assert_eq!(current_background_opacity_preview(), Some(preview(7, 1.0)));

        publish_background_opacity_preview(None);
        assert_eq!(current_background_opacity_preview(), None);
    }

    #[test]
    fn synced_preview_clears_when_saved_matches_preview() {
        let _guard = TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        assert_eq!(
            synced_background_opacity_preview(0.4, Some(preview(7, 0.4))),
            None
        );
        assert_eq!(
            synced_background_opacity_preview(0.4, Some(preview(7, 0.6))),
            Some(preview(7, 0.6))
        );
    }
}
