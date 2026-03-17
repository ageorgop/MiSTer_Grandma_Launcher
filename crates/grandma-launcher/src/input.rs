// SPDX-License-Identifier: GPL-3.0-or-later
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    Escape,
}

const INITIAL_REPEAT_DELAY: Duration = Duration::from_millis(400);
const REPEAT_INTERVAL: Duration = Duration::from_millis(150);

pub struct InputState {
    held_action: Option<Action>,
    held_since: Instant,
    last_repeat: Instant,
    has_sent_initial: bool,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            held_action: None,
            held_since: Instant::now(),
            last_repeat: Instant::now(),
            has_sent_initial: false,
        }
    }

    pub fn on_press(&mut self, action: Action) -> Option<Action> {
        self.held_action = Some(action);
        self.held_since = Instant::now();
        self.last_repeat = Instant::now();
        self.has_sent_initial = true;
        Some(action)
    }

    pub fn on_release(&mut self, action: Action) {
        if self.held_action == Some(action) {
            self.held_action = None;
            self.has_sent_initial = false;
        }
    }

    pub fn poll_repeat(&mut self) -> Option<Action> {
        let action = self.held_action?;
        if !self.has_sent_initial { return None; }

        let held_duration = self.held_since.elapsed();
        if held_duration < INITIAL_REPEAT_DELAY {
            return None;
        }

        if self.last_repeat.elapsed() >= REPEAT_INTERVAL {
            self.last_repeat = Instant::now();
            return Some(action);
        }

        None
    }
}

/// Check if an evdev device is a gamepad or keyboard we should read from.
/// Filters out HDMI CEC adapters, IR receivers, power buttons, and sensors.
#[cfg(target_os = "linux")]
pub fn is_usable_device(dev: &evdev::Device) -> bool {
    let events = dev.supported_events();

    if !events.contains(evdev::EventType::KEY) {
        return false;
    }

    if let Some(keys) = dev.supported_keys() {
        // BTN_SOUTH is BTN_GAMEPAD in kernel headers — same constant
        if keys.contains(evdev::KeyCode::BTN_SOUTH) {
            return true;
        }
        if keys.contains(evdev::KeyCode::KEY_UP)
            && keys.contains(evdev::KeyCode::KEY_DOWN)
        {
            return true;
        }
    }

    false
}

/// Normalize evdev key/axis events into logical actions.
#[cfg(target_os = "linux")]
pub fn normalize_key(code: evdev::KeyCode) -> Option<Action> {
    use evdev::KeyCode;
    match code {
        KeyCode::BTN_SOUTH => Some(Action::Confirm),
        KeyCode::BTN_EAST => Some(Action::Back),
        KeyCode::BTN_START => Some(Action::Escape),

        KeyCode::BTN_DPAD_UP => Some(Action::Up),
        KeyCode::BTN_DPAD_DOWN => Some(Action::Down),
        KeyCode::BTN_DPAD_LEFT => Some(Action::Left),
        KeyCode::BTN_DPAD_RIGHT => Some(Action::Right),

        KeyCode::KEY_UP => Some(Action::Up),
        KeyCode::KEY_DOWN => Some(Action::Down),
        KeyCode::KEY_LEFT => Some(Action::Left),
        KeyCode::KEY_RIGHT => Some(Action::Right),
        KeyCode::KEY_ENTER => Some(Action::Confirm),
        KeyCode::KEY_BACKSPACE => Some(Action::Back),
        KeyCode::KEY_ESC => Some(Action::Escape),

        _ => None,
    }
}

pub enum AxisEvent {
    Pressed(Action),
    Released(Action),
}

#[cfg(target_os = "linux")]
pub fn normalize_axis(
    code: evdev::AbsoluteAxisCode,
    value: i32,
    prev_y: &mut Option<Action>,
    prev_x: &mut Option<Action>,
) -> Option<AxisEvent> {
    use evdev::AbsoluteAxisCode;
    match code {
        AbsoluteAxisCode::ABS_HAT0Y | AbsoluteAxisCode::ABS_Y => {
            let new_action = if value < -16000 || value == -1 { Some(Action::Up) }
                else if value > 16000 || value == 1 { Some(Action::Down) }
                else { None };

            match (prev_y.take(), new_action) {
                (Some(old), None) => Some(AxisEvent::Released(old)),
                (Some(old), Some(new)) if old != new => {
                    *prev_y = Some(new);
                    Some(AxisEvent::Released(old))
                }
                (_, Some(new)) => {
                    *prev_y = Some(new);
                    Some(AxisEvent::Pressed(new))
                }
                (None, None) => None,
            }
        }
        AbsoluteAxisCode::ABS_HAT0X | AbsoluteAxisCode::ABS_X => {
            let new_action = if value < -16000 || value == -1 { Some(Action::Left) }
                else if value > 16000 || value == 1 { Some(Action::Right) }
                else { None };

            match (prev_x.take(), new_action) {
                (Some(old), None) => Some(AxisEvent::Released(old)),
                (Some(old), Some(new)) if old != new => {
                    *prev_x = Some(new);
                    Some(AxisEvent::Released(old))
                }
                (_, Some(new)) => {
                    *prev_x = Some(new);
                    Some(AxisEvent::Pressed(new))
                }
                (None, None) => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_press_fires_immediately() {
        let mut state = InputState::new();
        assert_eq!(state.on_press(Action::Confirm), Some(Action::Confirm));
    }

    #[test]
    fn test_no_repeat_before_delay() {
        let mut state = InputState::new();
        state.on_press(Action::Down);
        assert_eq!(state.poll_repeat(), None);
    }

    #[test]
    fn test_release_clears_held() {
        let mut state = InputState::new();
        state.on_press(Action::Down);
        state.on_release(Action::Down);
        assert_eq!(state.poll_repeat(), None);
    }
}
