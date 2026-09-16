//! Debounced push button (active low with the internal pull-up).

use embedded_hal::digital::InputPin;

/// How long a level must hold before it is accepted.
pub const DEBOUNCE_MS: u64 = 20;

/// Edge produced by [`Button::poll`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum ButtonEvent {
    Pressed,
    Released,
}

pub struct Button<P: InputPin> {
    pin: P,
    stable: bool,
    candidate: bool,
    candidate_since_ms: u64,
}

impl<P: InputPin> Button<P> {
    pub fn new(mut pin: P, now_ms: u64) -> Self {
        let pressed = pin.is_low().unwrap_or(false);
        Self {
            pin,
            stable: pressed,
            candidate: pressed,
            candidate_since_ms: now_ms,
        }
    }

    /// Instantaneous, undebounced state. Used for the power-up gesture.
    pub fn is_pressed_raw(&mut self) -> bool {
        self.pin.is_low().unwrap_or(false)
    }

    /// Samples the pin and reports a debounced edge, if any.
    pub fn poll(&mut self, now_ms: u64) -> Option<ButtonEvent> {
        let raw = self.is_pressed_raw();
        if raw != self.candidate {
            self.candidate = raw;
            self.candidate_since_ms = now_ms;
            return None;
        }
        if raw != self.stable && now_ms.wrapping_sub(self.candidate_since_ms) >= DEBOUNCE_MS {
            self.stable = raw;
            return Some(if raw {
                ButtonEvent::Pressed
            } else {
                ButtonEvent::Released
            });
        }
        None
    }
}
