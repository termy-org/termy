pub use termy_core::{TerminalKeyEventKind, TerminalKeyboardMode, TermyKeystroke, TermyModifiers};

pub fn keystroke_to_input(
    keystroke: &gpui::Keystroke,
    event_kind: TerminalKeyEventKind,
    keyboard_mode: TerminalKeyboardMode,
    prompt_shortcuts_enabled: bool,
) -> Option<Vec<u8>> {
    let keystroke = TermyKeystroke {
        modifiers: TermyModifiers {
            control: keystroke.modifiers.control,
            alt: keystroke.modifiers.alt,
            shift: keystroke.modifiers.shift,
            platform: keystroke.modifiers.platform,
            function: keystroke.modifiers.function,
        },
        key: keystroke.key.to_string(),
        key_char: keystroke.key_char.as_ref().map(ToString::to_string),
    };
    termy_core::keystroke_to_input(
        &keystroke,
        event_kind,
        keyboard_mode,
        prompt_shortcuts_enabled,
    )
}
