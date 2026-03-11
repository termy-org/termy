use std::sync::{LazyLock, Mutex};

static BACKGROUND_OPACITY_PREVIEW: LazyLock<Mutex<Option<f32>>> = LazyLock::new(|| Mutex::new(None));
static BACKGROUND_OPACITY_PREVIEW_SUBSCRIBERS: LazyLock<Mutex<Vec<flume::Sender<Option<f32>>>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn publish_background_opacity_preview(opacity: Option<f32>) {
    if let Ok(mut preview) = BACKGROUND_OPACITY_PREVIEW.lock() {
        *preview = opacity.map(|value| value.clamp(0.0, 1.0));
    }

    let Ok(mut subscribers) = BACKGROUND_OPACITY_PREVIEW_SUBSCRIBERS.lock() else {
        return;
    };
    #[cfg(test)]
    subscribers.retain(test_subscriber_is_alive);

    #[cfg(not(test))]
    subscribers.retain(|tx| tx.send(current_background_opacity_preview()).is_ok());
}

#[cfg(test)]
fn test_subscriber_is_alive(tx: &flume::Sender<Option<f32>>) -> bool {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    catch_unwind(AssertUnwindSafe(|| tx.send(current_background_opacity_preview())))
        .ok()
        .is_some_and(|result| result.is_ok())
}

pub fn subscribe_background_opacity_preview() -> flume::Receiver<Option<f32>> {
    let (tx, rx) = flume::unbounded();
    if let Ok(mut subscribers) = BACKGROUND_OPACITY_PREVIEW_SUBSCRIBERS.lock() {
        subscribers.push(tx);
    }
    rx
}

pub fn current_background_opacity_preview() -> Option<f32> {
    BACKGROUND_OPACITY_PREVIEW
        .lock()
        .ok()
        .and_then(|preview| *preview)
}

pub fn effective_background_opacity(saved_opacity: f32, preview_opacity: Option<f32>) -> f32 {
    preview_opacity.unwrap_or(saved_opacity).clamp(0.0, 1.0)
}

pub fn synced_background_opacity_preview(
    saved_opacity: f32,
    preview_opacity: Option<f32>,
) -> Option<f32> {
    let saved_opacity = saved_opacity.clamp(0.0, 1.0);
    preview_opacity
        .map(|value| value.clamp(0.0, 1.0))
        .filter(|value| (*value - saved_opacity).abs() >= f32::EPSILON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    static TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn publishing_preview_notifies_subscribers() {
        let _guard = TEST_LOCK.lock().expect("preview test lock");
        publish_background_opacity_preview(None);
        let rx = subscribe_background_opacity_preview();

        publish_background_opacity_preview(Some(0.45));

        assert_eq!(rx.recv().expect("preview notification"), Some(0.45));
    }

    #[test]
    fn publishing_none_clears_preview() {
        let _guard = TEST_LOCK.lock().expect("preview test lock");
        publish_background_opacity_preview(Some(0.45));
        let rx = subscribe_background_opacity_preview();

        publish_background_opacity_preview(None);

        assert_eq!(rx.recv().expect("preview clear notification"), None);
        assert_eq!(current_background_opacity_preview(), None);
    }

    #[test]
    fn current_preview_reflects_latest_value() {
        let _guard = TEST_LOCK.lock().expect("preview test lock");
        publish_background_opacity_preview(Some(0.2));
        assert_eq!(current_background_opacity_preview(), Some(0.2));

        publish_background_opacity_preview(Some(1.5));
        assert_eq!(current_background_opacity_preview(), Some(1.0));

        publish_background_opacity_preview(None);
        assert_eq!(current_background_opacity_preview(), None);
    }

    #[test]
    fn synced_preview_clears_when_saved_matches_preview() {
        let _guard = TEST_LOCK.lock().expect("preview test lock");
        assert_eq!(synced_background_opacity_preview(0.4, Some(0.4)), None);
        assert_eq!(
            synced_background_opacity_preview(0.4, Some(0.6)),
            Some(0.6)
        );
    }
}
