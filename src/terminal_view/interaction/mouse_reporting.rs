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

    fn encode_mouse_packet_for_pane(
        &self,
        pane_id: &str,
        event_kind: TerminalMouseEventKind,
        cell: CellPos,
        modifiers: gpui::Modifiers,
    ) -> Option<Vec<u8>> {
        let mode = self.pane_mouse_mode(pane_id)?;
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
    ) -> bool {
        let Some(packet) = self.encode_mouse_packet_for_pane(pane_id, event_kind, cell, modifiers)
        else {
            return false;
        };
        self.send_mouse_packet_to_pane(pane_id, packet.as_slice())
    }

    fn set_pressed_mouse_target(&mut self, button: MouseTrackedButton, target: MouseReportTargetCell) {
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

        let sent = self.try_send_mouse_event_to_pane(
            pane_id.as_str(),
            TerminalMouseEventKind::Press(button.terminal_button()),
            cell,
            event.modifiers,
        );
        if !sent {
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
        if self.runtime_kind() == RuntimeKind::Tmux && !self.is_active_pane_id(pane_id.as_str()) {
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
        if is_mouse_reporting_bypass(event.modifiers) {
            return false;
        }

        let send_result = if let Some(pressed_button) = event.pressed_button {
            let Some(button) = MouseTrackedButton::from_mouse_button(pressed_button) else {
                return false;
            };
            let Some(tracked) = self.pressed_mouse_target(button) else {
                return false;
            };
            let cell = self
                .position_to_cell_in_pane(tracked.pane_id.as_str(), event.position, true)
                .unwrap_or(CellPos {
                    col: tracked.col,
                    row: tracked.row,
                });
            let pane_id = tracked.pane_id.clone();
            let sent = self.try_send_mouse_event_to_pane(
                pane_id.as_str(),
                TerminalMouseEventKind::Drag(button.terminal_button()),
                cell,
                event.modifiers,
            );
            if sent {
                self.set_pressed_mouse_target(
                    button,
                    MouseReportTargetCell {
                        pane_id,
                        col: cell.col,
                        row: cell.row,
                    },
                );
            }
            sent
        } else {
            let Some((pane_id, cell)) = self.position_to_pane_cell(event.position, false) else {
                return false;
            };
            self.try_send_mouse_event_to_pane(
                pane_id.as_str(),
                TerminalMouseEventKind::Move,
                cell,
                event.modifiers,
            )
        };

        if send_result {
            cx.stop_propagation();
        }
        send_result
    }

    pub(in super::super) fn try_forward_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if is_mouse_reporting_bypass(event.modifiers) {
            return false;
        }

        let Some(button) = MouseTrackedButton::from_mouse_button(event.button) else {
            return false;
        };
        let Some(tracked) = self.take_pressed_mouse_target(button) else {
            return false;
        };
        if self.pane_terminal_by_id(tracked.pane_id.as_str()).is_none() {
            return false;
        }
        let cell = self
            .position_to_cell_in_pane(tracked.pane_id.as_str(), event.position, true)
            .unwrap_or(CellPos {
                col: tracked.col,
                row: tracked.row,
            });

        let sent = self.try_send_mouse_event_to_pane(
            tracked.pane_id.as_str(),
            TerminalMouseEventKind::Release(button.terminal_button()),
            cell,
            event.modifiers,
        );
        if sent {
            cx.stop_propagation();
        }
        sent
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
                cx.stop_propagation();
                true
            }
            TouchPhase::Ended => {
                self.mouse_reporting.scroll_accumulator_x = 0.0;
                self.mouse_reporting.scroll_accumulator_y = 0.0;
                cx.stop_propagation();
                true
            }
            TouchPhase::Moved => {
                let Some(terminal) = self.pane_terminal_by_id(pane_id.as_str()) else {
                    return false;
                };
                let size = terminal.size();
                let cell_width: f32 = size.cell_width.into();
                let cell_height: f32 = size.cell_height.into();
                if cell_width <= f32::EPSILON || cell_height <= f32::EPSILON {
                    return false;
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

                let mut sent_any = false;
                if vertical_steps > 0 {
                    let event_kind = if delta_y > 0.0 {
                        TerminalMouseEventKind::WheelUp
                    } else {
                        TerminalMouseEventKind::WheelDown
                    };
                    if let Some(packet) = self.encode_mouse_packet_for_pane(
                        pane_id.as_str(),
                        event_kind,
                        cell,
                        event.modifiers,
                    ) {
                        for _ in 0..vertical_steps {
                            if !self.send_mouse_packet_to_pane(pane_id.as_str(), packet.as_slice()) {
                                break;
                            }
                            sent_any = true;
                        }
                    }
                }

                if horizontal_steps > 0 {
                    let event_kind = if delta_x > 0.0 {
                        TerminalMouseEventKind::WheelLeft
                    } else {
                        TerminalMouseEventKind::WheelRight
                    };
                    if let Some(packet) = self.encode_mouse_packet_for_pane(
                        pane_id.as_str(),
                        event_kind,
                        cell,
                        event.modifiers,
                    ) {
                        for _ in 0..horizontal_steps {
                            if !self.send_mouse_packet_to_pane(pane_id.as_str(), packet.as_slice()) {
                                break;
                            }
                            sent_any = true;
                        }
                    }
                }

                if sent_any || vertical_steps == 0 && horizontal_steps == 0 {
                    cx.stop_propagation();
                    return true;
                }

                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    #[test]
    fn shift_is_mouse_reporting_bypass() {
        let modifiers = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert!(is_mouse_reporting_bypass(modifiers));
        assert!(!is_mouse_reporting_bypass(Modifiers::default()));
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
}
