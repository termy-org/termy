//! Window background appearance and system theme reactions for `TerminalView`.
//!
//! E1 tranche 1 — extracted from `mod.rs` per `docs/engineering/terminal-view-decomposition.md`.

use crate::config::{self, AppConfig};
use gpui::{Context, Window, WindowAppearance, WindowBackgroundAppearance};

use super::TerminalView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum BackgroundPlatform {
    MacOs,
    Windows,
    Linux,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BackgroundSupportContext {
    pub(super) platform: BackgroundPlatform,
    pub(super) linux_wayland_session: bool,
}

impl BackgroundSupportContext {
    pub(super) fn current() -> Self {
        #[cfg(target_os = "macos")]
        let platform = BackgroundPlatform::MacOs;
        #[cfg(target_os = "windows")]
        let platform = BackgroundPlatform::Windows;
        #[cfg(target_os = "linux")]
        let platform = BackgroundPlatform::Linux;
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let platform = BackgroundPlatform::Other;

        #[cfg(target_os = "linux")]
        let linux_wayland_session = std::env::var("XDG_SESSION_TYPE")
            .ok()
            .is_some_and(|session_type| session_type.eq_ignore_ascii_case("wayland"))
            || std::env::var_os("WAYLAND_DISPLAY").is_some();
        #[cfg(not(target_os = "linux"))]
        let linux_wayland_session = false;

        Self {
            platform,
            linux_wayland_session,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BlurFallbackReason {
    None,
    KnownUnsupported,
    UnknownSupport,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ResolvedBackgroundAppearance {
    pub(super) appearance: WindowBackgroundAppearance,
    pub(super) blur_fallback: BlurFallbackReason,
}

pub(super) fn resolve_background_appearance(
    background_opacity: f32,
    background_blur: bool,
    context: BackgroundSupportContext,
) -> ResolvedBackgroundAppearance {
    let opacity = background_opacity.clamp(0.0, 1.0);
    if opacity >= 1.0 {
        return ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Opaque,
            blur_fallback: BlurFallbackReason::None,
        };
    }

    if !background_blur {
        return ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Transparent,
            blur_fallback: BlurFallbackReason::None,
        };
    }

    match context.platform {
        BackgroundPlatform::MacOs | BackgroundPlatform::Windows => ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Blurred,
            blur_fallback: BlurFallbackReason::None,
        },
        BackgroundPlatform::Linux => {
            if context.linux_wayland_session {
                ResolvedBackgroundAppearance {
                    appearance: WindowBackgroundAppearance::Blurred,
                    blur_fallback: BlurFallbackReason::UnknownSupport,
                }
            } else {
                ResolvedBackgroundAppearance {
                    appearance: WindowBackgroundAppearance::Transparent,
                    blur_fallback: BlurFallbackReason::KnownUnsupported,
                }
            }
        }
        BackgroundPlatform::Other => ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Blurred,
            blur_fallback: BlurFallbackReason::UnknownSupport,
        },
    }
}

pub(crate) fn initial_window_background_appearance(
    config: &AppConfig,
) -> WindowBackgroundAppearance {
    resolve_background_appearance(
        config.background_opacity,
        config.background_blur,
        BackgroundSupportContext::current(),
    )
    .appearance
}

impl TerminalView {
    pub(super) fn sync_window_background_appearance(&mut self, window: &mut Window) {
        let resolved = resolve_background_appearance(
            self.effective_background_opacity(),
            self.background_blur,
            self.background_support_context,
        );

        if self.last_window_background_appearance != Some(resolved.appearance) {
            window.set_background_appearance(resolved.appearance);
            self.last_window_background_appearance = Some(resolved.appearance);
        }

        if self.background_blur
            && resolved.blur_fallback == BlurFallbackReason::KnownUnsupported
            && !self.warned_blur_unsupported_once
        {
            self.warned_blur_unsupported_once = true;
            termy_toast::warning(
                "Background blur is unsupported in this session; using transparency",
            );
        }
    }

    pub(super) fn handle_window_appearance_change(
        &mut self,
        appearance: WindowAppearance,
        cx: &mut Context<Self>,
    ) {
        let next = config::system_appearance_from_window(appearance);
        if self.system_appearance == next {
            return;
        }
        self.system_appearance = next;
        if self.theme_mode != config::AppearanceMode::Manual {
            self.reapply_theme(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;

    #[test]
    fn resolve_background_appearance_is_opaque_when_opacity_is_full() {
        let resolved = resolve_background_appearance(
            1.0,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::MacOs,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Opaque);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::None);
    }

    #[test]
    fn resolve_background_appearance_is_transparent_without_blur() {
        let resolved = resolve_background_appearance(
            0.85,
            false,
            BackgroundSupportContext {
                platform: BackgroundPlatform::Windows,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Transparent);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::None);
    }

    #[test]
    fn resolve_background_appearance_blur_is_known_unsupported_on_linux_non_wayland() {
        let resolved = resolve_background_appearance(
            0.9,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::Linux,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Transparent);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::KnownUnsupported);
    }

    #[test]
    fn resolve_background_appearance_blur_is_unknown_on_linux_wayland() {
        let resolved = resolve_background_appearance(
            0.9,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::Linux,
                linux_wayland_session: true,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Blurred);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::UnknownSupport);
    }

    #[test]
    fn resolve_background_appearance_blur_is_enabled_on_macos() {
        let resolved = resolve_background_appearance(
            0.9,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::MacOs,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Blurred);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::None);
    }

    #[test]
    fn resolve_background_appearance_uses_preview_opacity_during_drag() {
        let effective_opacity = config::effective_background_opacity(
            1.0,
            Some(config::BackgroundOpacityPreview {
                owner_id: 1,
                opacity: 0.4,
            }),
        );
        let resolved = resolve_background_appearance(
            effective_opacity,
            false,
            BackgroundSupportContext {
                platform: BackgroundPlatform::MacOs,
                linux_wayland_session: false,
            },
        );

        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Transparent);
    }
}
