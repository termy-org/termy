use super::*;
use termy_terminal_ui::{
    TerminalMouseButton, TerminalMouseEventKind, TerminalMouseModifiers, TerminalMousePosition,
    encode_mouse_report,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseTrackedButton {
    Left,
    Middle,
    Right,
}
const TRACKED_MOUSE_BUTTONS: [MouseTrackedButton; 3] = [
    MouseTrackedButton::Left,
    MouseTrackedButton::Middle,
    MouseTrackedButton::Right,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseForwardOutcome {
    NotHandled,
    Consumed,
    Sent,
}

impl MouseForwardOutcome {
    const fn is_handled(self) -> bool {
        !matches!(self, Self::NotHandled)
    }
}

impl MouseTrackedButton {
    fn from_mouse_button(button: MouseButton) -> Option<Self> {
        match button {
            MouseButton::Left => Some(Self::Left),
            MouseButton::Middle => Some(Self::Middle),
            MouseButton::Right => Some(Self::Right),
            MouseButton::Navigate(_) => None,
        }
    }

    fn terminal_button(self) -> TerminalMouseButton {
        match self {
            Self::Left => TerminalMouseButton::Left,
            Self::Middle => TerminalMouseButton::Middle,
            Self::Right => TerminalMouseButton::Right,
        }
    }
}

fn is_mouse_reporting_bypass(modifiers: gpui::Modifiers) -> bool {
    modifiers.shift
}

fn should_skip_mouse_reporting_for_bypass(
    modifiers: gpui::Modifiers,
    has_forwarded_press: bool,
) -> bool {
    is_mouse_reporting_bypass(modifiers) && !has_forwarded_press
}

fn terminal_mouse_modifiers(modifiers: gpui::Modifiers) -> TerminalMouseModifiers {
    TerminalMouseModifiers {
        shift: modifiers.shift,
        alt: modifiers.alt,
        control: modifiers.control,
    }
}

fn quantized_scroll_steps(accumulated: &mut f32, delta_pixels: f32, cell_extent: f32) -> usize {
    if cell_extent <= f32::EPSILON {
        return 0;
    }

    *accumulated += delta_pixels;
    let steps = (*accumulated / cell_extent).abs() as usize;
    *accumulated %= cell_extent;
    steps
}

fn mouse_forward_outcome(
    mode: TerminalMouseMode,
    packet_send_result: Option<bool>,
) -> MouseForwardOutcome {
    if !mode.enabled {
        return MouseForwardOutcome::NotHandled;
    }

    // When mouse mode is enabled, encode/send failures are still consumed so
    // local selection/scroll handlers cannot run on the same event.
    match packet_send_result {
        Some(true) => MouseForwardOutcome::Sent,
        Some(false) | None => MouseForwardOutcome::Consumed,
    }
}

fn should_emit_drag_report(previous: &MouseReportTargetCell, next: CellPos) -> bool {
    previous.col != next.col || previous.row != next.row
}

fn should_emit_move_report(
    previous: Option<&MouseReportTargetCell>,
    pane_id: &str,
    next: CellPos,
) -> bool {
    let Some(previous) = previous else {
        return true;
    };

    previous.pane_id != pane_id || previous.col != next.col || previous.row != next.row
}

fn should_focus_target_after_mouse_press(
    outcome: MouseForwardOutcome,
    target_is_active: bool,
) -> bool {
    outcome.is_handled() && !target_is_active
}

fn mouse_pressed_target_ref(
    state: &MouseReportingState,
    button: MouseTrackedButton,
) -> Option<&MouseReportTargetCell> {
    match button {
        MouseTrackedButton::Left => state.left_button.as_ref(),
        MouseTrackedButton::Middle => state.middle_button.as_ref(),
        MouseTrackedButton::Right => state.right_button.as_ref(),
    }
}

fn mouse_pressed_target_slot(
    state: &mut MouseReportingState,
    button: MouseTrackedButton,
) -> &mut Option<MouseReportTargetCell> {
    match button {
        MouseTrackedButton::Left => &mut state.left_button,
        MouseTrackedButton::Middle => &mut state.middle_button,
        MouseTrackedButton::Right => &mut state.right_button,
    }
}

fn has_forwarded_press(state: &MouseReportingState) -> bool {
    state.left_button.is_some() || state.middle_button.is_some() || state.right_button.is_some()
}

impl TerminalView {
    fn pane_mouse_mode(&self, pane_id: &str) -> Option<TerminalMouseMode> {
        self.pane_terminal_by_id(pane_id).map(Terminal::mouse_mode)
    }

    fn send_mouse_packet_to_pane(&self, pane_id: &str, packet: &[u8]) -> bool {
        if packet.is_empty() {
            return false;
        }

        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_send_input_to_pane(pane_id, packet),
            RuntimeKind::Native => {
                let Some(terminal) = self.pane_terminal_by_id(pane_id) else {
                    return false;
                };
                terminal.write_input(packet);
                true
            }
        }
    }

    fn encode_mouse_packet(
        mode: TerminalMouseMode,
        event_kind: TerminalMouseEventKind,
        cell: CellPos,
        modifiers: gpui::Modifiers,
    ) -> Option<Vec<u8>> {
        encode_mouse_report(
            mode,
            event_kind,
            TerminalMousePosition {
                col: cell.col,
                row: cell.row,
            },
            terminal_mouse_modifiers(modifiers),
        )
    }

    fn try_send_mouse_event_to_pane(
        &self,
        pane_id: &str,
        event_kind: TerminalMouseEventKind,
        cell: CellPos,
        modifiers: gpui::Modifiers,
    ) -> MouseForwardOutcome {
        let Some(mode) = self.pane_mouse_mode(pane_id) else {
            return MouseForwardOutcome::NotHandled;
        };

        let send_result = Self::encode_mouse_packet(mode, event_kind, cell, modifiers)
            .map(|packet| self.send_mouse_packet_to_pane(pane_id, packet.as_slice()));
        mouse_forward_outcome(mode, send_result)
    }

    fn set_pressed_mouse_target(
        &mut self,
        button: MouseTrackedButton,
        target: MouseReportTargetCell,
    ) {
        *mouse_pressed_target_slot(&mut self.mouse_reporting, button) = Some(target);
    }

    fn take_pressed_mouse_target(
        &mut self,
        button: MouseTrackedButton,
    ) -> Option<MouseReportTargetCell> {
        mouse_pressed_target_slot(&mut self.mouse_reporting, button).take()
    }

    fn pressed_mouse_target(&self, button: MouseTrackedButton) -> Option<&MouseReportTargetCell> {
        mouse_pressed_target_ref(&self.mouse_reporting, button)
    }

    fn has_forwarded_mouse_press(&self) -> bool {
        has_forwarded_press(&self.mouse_reporting)
    }

    fn send_synthetic_mouse_release(
        &self,
        button: MouseTrackedButton,
        target: &MouseReportTargetCell,
    ) {
        let _ = self.try_send_mouse_event_to_pane(
            target.pane_id.as_str(),
            TerminalMouseEventKind::Release(button.terminal_button()),
            CellPos {
                col: target.col,
                row: target.row,
            },
            gpui::Modifiers::default(),
        );
    }

    fn target_pane_is_missing(&self, target: &MouseReportTargetCell) -> bool {
        self.pane_terminal_by_id(target.pane_id.as_str()).is_none()
    }

    fn drop_missing_forwarded_mouse_targets(&mut self) {
        for button in TRACKED_MOUSE_BUTTONS {
            let missing = self
                .pressed_mouse_target(button)
                .is_some_and(|target| self.target_pane_is_missing(target));
            if missing {
                self.take_pressed_mouse_target(button);
            }
        }
        let hover_missing = self
            .mouse_reporting
            .hover_target
            .as_ref()
            .is_some_and(|target| self.target_pane_is_missing(target));
        if hover_missing {
            self.mouse_reporting.hover_target = None;
        }
    }

    pub(crate) fn release_forwarded_mouse_presses_for_panes(
        &mut self,
        pane_ids: &[String],
    ) -> bool {
        if pane_ids.is_empty() {
            return false;
        }

        let mut changed = false;
        for button in TRACKED_MOUSE_BUTTONS {
            let should_release = self.pressed_mouse_target(button).is_some_and(|target| {
                pane_ids
                    .iter()
                    .any(|pane_id| pane_id.as_str() == target.pane_id.as_str())
            });
            if !should_release {
                continue;
            }

            if let Some(target) = self.take_pressed_mouse_target(button) {
                self.send_synthetic_mouse_release(button, &target);
                changed = true;
            }
        }
        let clear_hover = self
            .mouse_reporting
            .hover_target
            .as_ref()
            .is_some_and(|target| {
                pane_ids
                    .iter()
                    .any(|pane_id| pane_id.as_str() == target.pane_id.as_str())
            });
        if clear_hover {
            self.mouse_reporting.hover_target = None;
            changed = true;
        }

        changed
    }

    pub(crate) fn release_all_forwarded_mouse_presses(&mut self) -> bool {
        let mut changed = false;
        for button in TRACKED_MOUSE_BUTTONS {
            if let Some(target) = self.take_pressed_mouse_target(button) {
                self.send_synthetic_mouse_release(button, &target);
                changed = true;
            }
        }
        if self.mouse_reporting.hover_target.take().is_some() {
            changed = true;
        }
        changed
    }

    pub(in super::super) fn try_forward_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_mouse_reporting_bypass(event.modifiers) {
            return false;
        }

        let Some(button) = MouseTrackedButton::from_mouse_button(event.button) else {
            return false;
        };
        let Some((pane_id, cell)) = self.position_to_pane_cell(event.position, false) else {
            return false;
        };

        let outcome = self.try_send_mouse_event_to_pane(
            pane_id.as_str(),
            TerminalMouseEventKind::Press(button.terminal_button()),
            cell,
            event.modifiers,
        );
        if !outcome.is_handled() {
            return false;
        }

        self.set_pressed_mouse_target(
            button,
            MouseReportTargetCell {
                pane_id: pane_id.clone(),
                col: cell.col,
                row: cell.row,
            },
        );
        self.mouse_reporting.hover_target = None;
        if should_focus_target_after_mouse_press(outcome, self.is_active_pane_id(pane_id.as_str()))
        {
            let _ = self.focus_pane_target(pane_id.as_str(), cx);
        }
        cx.stop_propagation();
        true
    }

    pub(in super::super) fn try_forward_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.drop_missing_forwarded_mouse_targets();
        if should_skip_mouse_reporting_for_bypass(event.modifiers, self.has_forwarded_mouse_press())
        {
            return false;
        }

        let outcome = if let Some(pressed_button) = event.pressed_button {
            let Some(button) = MouseTrackedButton::from_mouse_button(pressed_button) else {
                return false;
            };
            let Some(tracked) = self.pressed_mouse_target(button).cloned() else {
                return false;
            };
            self.mouse_reporting.hover_target = None;
            let cell = self
                .position_to_cell_in_pane(tracked.pane_id.as_str(), event.position, true)
                .unwrap_or(CellPos {
                    col: tracked.col,
                    row: tracked.row,
                });

            if !should_emit_drag_report(&tracked, cell) {
                self.pane_mouse_mode(tracked.pane_id.as_str())
                    .map_or(MouseForwardOutcome::NotHandled, |mode| {
                        mouse_forward_outcome(mode, None)
                    })
            } else {
                let outcome = self.try_send_mouse_event_to_pane(
                    tracked.pane_id.as_str(),
                    TerminalMouseEventKind::Drag(button.terminal_button()),
                    cell,
                    event.modifiers,
                );
                if outcome.is_handled() {
                    self.set_pressed_mouse_target(
                        button,
                        MouseReportTargetCell {
                            pane_id: tracked.pane_id,
                            col: cell.col,
                            row: cell.row,
                        },
                    );
                }
                outcome
            }
        } else {
            let Some((pane_id, cell)) = self.position_to_pane_cell(event.position, false) else {
                self.mouse_reporting.hover_target = None;
                return false;
            };
            if !should_emit_move_report(
                self.mouse_reporting.hover_target.as_ref(),
                pane_id.as_str(),
                cell,
            ) {
                let outcome = self
                    .pane_mouse_mode(pane_id.as_str())
                    .map_or(MouseForwardOutcome::NotHandled, |mode| {
                        mouse_forward_outcome(mode, None)
                    });
                if outcome.is_handled() {
                    self.mouse_reporting.hover_target = Some(MouseReportTargetCell {
                        pane_id,
                        col: cell.col,
                        row: cell.row,
                    });
                } else {
                    self.mouse_reporting.hover_target = None;
                }
                outcome
            } else {
                let outcome = self.try_send_mouse_event_to_pane(
                    pane_id.as_str(),
                    TerminalMouseEventKind::Move,
                    cell,
                    event.modifiers,
                );
                if outcome.is_handled() {
                    self.mouse_reporting.hover_target = Some(MouseReportTargetCell {
                        pane_id,
                        col: cell.col,
                        row: cell.row,
                    });
                } else {
                    self.mouse_reporting.hover_target = None;
                }
                outcome
            }
        };

        if outcome.is_handled() {
            cx.stop_propagation();
            return true;
        }
        false
    }

    pub(in super::super) fn try_forward_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.drop_missing_forwarded_mouse_targets();
        if should_skip_mouse_reporting_for_bypass(event.modifiers, self.has_forwarded_mouse_press())
        {
            return false;
        }

        let Some(button) = MouseTrackedButton::from_mouse_button(event.button) else {
            return false;
        };
        let Some(tracked) = self.take_pressed_mouse_target(button) else {
            return false;
        };
        self.mouse_reporting.hover_target = None;
        if self.pane_terminal_by_id(tracked.pane_id.as_str()).is_none() {
            return false;
        }
        let cell = self
            .position_to_cell_in_pane(tracked.pane_id.as_str(), event.position, true)
            .unwrap_or(CellPos {
                col: tracked.col,
                row: tracked.row,
            });

        let outcome = self.try_send_mouse_event_to_pane(
            tracked.pane_id.as_str(),
            TerminalMouseEventKind::Release(button.terminal_button()),
            cell,
            event.modifiers,
        );
        if outcome.is_handled() {
            cx.stop_propagation();
            return true;
        }
        false
    }

    pub(in super::super) fn try_forward_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_mouse_reporting_bypass(event.modifiers) {
            return false;
        }

        let Some((pane_id, cell)) = self.position_to_pane_cell(event.position, false) else {
            return false;
        };
        let Some(mode) = self.pane_mouse_mode(pane_id.as_str()) else {
            return false;
        };
        if !mode.enabled {
            return false;
        }

        match event.touch_phase {
            TouchPhase::Started => {
                self.mouse_reporting.scroll_accumulator_x = 0.0;
                self.mouse_reporting.scroll_accumulator_y = 0.0;
            }
            TouchPhase::Ended => {
                self.mouse_reporting.scroll_accumulator_x = 0.0;
                self.mouse_reporting.scroll_accumulator_y = 0.0;
            }
            TouchPhase::Moved => {
                let Some(terminal) = self.pane_terminal_by_id(pane_id.as_str()) else {
                    cx.stop_propagation();
                    return true;
                };
                let size = terminal.size();
                let cell_width: f32 = size.cell_width.into();
                let cell_height: f32 = size.cell_height.into();
                if cell_width <= f32::EPSILON || cell_height <= f32::EPSILON {
                    cx.stop_propagation();
                    return true;
                }

                let delta = event.delta.pixel_delta(size.cell_height);
                let delta_x: f32 = delta.x.into();
                let delta_y: f32 = delta.y.into();
                let vertical_steps = quantized_scroll_steps(
                    &mut self.mouse_reporting.scroll_accumulator_y,
                    delta_y,
                    cell_height,
                );
                let horizontal_steps = quantized_scroll_steps(
                    &mut self.mouse_reporting.scroll_accumulator_x,
                    delta_x,
                    cell_width,
                );

                if vertical_steps > 0 {
                    let event_kind = if delta_y > 0.0 {
                        TerminalMouseEventKind::WheelUp
                    } else {
                        TerminalMouseEventKind::WheelDown
                    };
                    if let Some(packet) =
                        Self::encode_mouse_packet(mode, event_kind, cell, event.modifiers)
                    {
                        for _ in 0..vertical_steps {
                            if !self.send_mouse_packet_to_pane(pane_id.as_str(), packet.as_slice())
                            {
                                break;
                            }
                        }
                    }
                }

                if horizontal_steps > 0 {
                    let event_kind = if delta_x > 0.0 {
                        TerminalMouseEventKind::WheelLeft
                    } else {
                        TerminalMouseEventKind::WheelRight
                    };
                    if let Some(packet) =
                        Self::encode_mouse_packet(mode, event_kind, cell, event.modifiers)
                    {
                        for _ in 0..horizontal_steps {
                            if !self.send_mouse_packet_to_pane(pane_id.as_str(), packet.as_slice())
                            {
                                break;
                            }
                        }
                    }
                }
            }
        }

        cx.stop_propagation();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;
    use termy_terminal_ui::TerminalMouseMode;

    fn enabled_mode() -> TerminalMouseMode {
        TerminalMouseMode {
            enabled: true,
            ..TerminalMouseMode::default()
        }
    }

    #[test]
    fn shift_is_mouse_reporting_bypass() {
        let modifiers = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert!(is_mouse_reporting_bypass(modifiers));
        assert!(!is_mouse_reporting_bypass(Modifiers::default()));
        assert!(should_skip_mouse_reporting_for_bypass(modifiers, false));
        assert!(!should_skip_mouse_reporting_for_bypass(modifiers, true));
    }

    #[test]
    fn pressed_target_state_round_trips_for_each_button() {
        let mut state = MouseReportingState::default();
        let target = MouseReportTargetCell {
            pane_id: "%1".to_string(),
            col: 3,
            row: 7,
        };
        *mouse_pressed_target_slot(&mut state, MouseTrackedButton::Left) = Some(target.clone());
        assert_eq!(
            mouse_pressed_target_ref(&state, MouseTrackedButton::Left),
            Some(&target)
        );
        let taken = mouse_pressed_target_slot(&mut state, MouseTrackedButton::Left).take();
        assert_eq!(taken, Some(target));
        assert!(mouse_pressed_target_ref(&state, MouseTrackedButton::Left).is_none());
    }

    #[test]
    fn quantized_scroll_steps_emits_multiple_steps_when_delta_exceeds_cell_extent() {
        let mut accumulated = 0.0;
        let steps = quantized_scroll_steps(&mut accumulated, 75.0, 24.0);
        assert_eq!(steps, 3);
    }

    #[test]
    fn quantized_scroll_steps_accumulates_fractional_motion() {
        let mut accumulated = 0.0;
        assert_eq!(quantized_scroll_steps(&mut accumulated, 8.0, 24.0), 0);
        assert_eq!(quantized_scroll_steps(&mut accumulated, 8.0, 24.0), 0);
        assert_eq!(quantized_scroll_steps(&mut accumulated, 8.0, 24.0), 1);
    }

    #[test]
    fn mouse_forward_outcome_returns_not_handled_when_mode_disabled() {
        assert_eq!(
            mouse_forward_outcome(TerminalMouseMode::default(), Some(true)),
            MouseForwardOutcome::NotHandled
        );
    }

    #[test]
    fn mouse_forward_outcome_returns_consumed_when_packet_is_missing() {
        assert_eq!(
            mouse_forward_outcome(enabled_mode(), None),
            MouseForwardOutcome::Consumed
        );
    }

    #[test]
    fn mouse_forward_outcome_returns_sent_when_packet_send_succeeds() {
        assert_eq!(
            mouse_forward_outcome(enabled_mode(), Some(true)),
            MouseForwardOutcome::Sent
        );
    }

    #[test]
    fn drag_reports_are_suppressed_when_cell_has_not_changed() {
        let target = MouseReportTargetCell {
            pane_id: "%1".to_string(),
            col: 3,
            row: 7,
        };
        assert!(!should_emit_drag_report(
            &target,
            CellPos { col: 3, row: 7 }
        ));
    }

    #[test]
    fn drag_reports_emit_when_cell_changes() {
        let target = MouseReportTargetCell {
            pane_id: "%1".to_string(),
            col: 3,
            row: 7,
        };
        assert!(should_emit_drag_report(&target, CellPos { col: 4, row: 7 }));
    }

    #[test]
    fn move_reports_are_suppressed_when_target_cell_and_pane_match() {
        let target = MouseReportTargetCell {
            pane_id: "%1".to_string(),
            col: 3,
            row: 7,
        };
        assert!(!should_emit_move_report(
            Some(&target),
            "%1",
            CellPos { col: 3, row: 7 }
        ));
    }

    #[test]
    fn move_reports_emit_when_target_changes() {
        let target = MouseReportTargetCell {
            pane_id: "%1".to_string(),
            col: 3,
            row: 7,
        };
        assert!(should_emit_move_report(
            Some(&target),
            "%2",
            CellPos { col: 3, row: 7 }
        ));
    }

    #[test]
    fn focus_handoff_runs_for_any_handled_press_on_inactive_target() {
        assert!(should_focus_target_after_mouse_press(
            MouseForwardOutcome::Sent,
            false
        ));
        assert!(should_focus_target_after_mouse_press(
            MouseForwardOutcome::Consumed,
            false
        ));
        assert!(!should_focus_target_after_mouse_press(
            MouseForwardOutcome::NotHandled,
            false
        ));
        assert!(!should_focus_target_after_mouse_press(
            MouseForwardOutcome::Sent,
            true
        ));
    }
}
