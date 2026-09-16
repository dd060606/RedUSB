//! RGB status LED driven by three PWM channels (common anode, active low).

use embedded_hal::pwm::SetDutyCycle;
use rp2040_hal::gpio::bank0::{Gpio2, Gpio3, Gpio4};
use rp2040_hal::gpio::{FunctionNull, FunctionPwm, Pin, PullDown};
use rp2040_hal::pwm::{FreeRunning, Pwm1, Pwm2, Slice};

/// 8-bit RGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const OFF: Color = Color::new(0, 0, 0);
    pub const RED: Color = Color::new(255, 0, 0);
    pub const GREEN: Color = Color::new(0, 255, 0);
    pub const BLUE: Color = Color::new(0, 0, 255);
    pub const YELLOW: Color = Color::new(255, 200, 0);
    pub const CYAN: Color = Color::new(0, 255, 255);
    pub const MAGENTA: Color = Color::new(255, 0, 255);
    pub const WHITE: Color = Color::new(255, 255, 255);
    pub const ORANGE: Color = Color::new(255, 60, 0);

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Multiplies every channel by `num / den`.
    pub const fn scaled(self, num: u32, den: u32) -> Self {
        Self::new(
            (self.r as u32 * num / den) as u8,
            (self.g as u32 * num / den) as u8,
            (self.b as u32 * num / den) as u8,
        )
    }
}

/// Colours assigned to script slots, in order.
pub const PALETTE: [Color; 8] = [
    Color::RED,
    Color::GREEN,
    Color::BLUE,
    Color::YELLOW,
    Color::CYAN,
    Color::MAGENTA,
    Color::WHITE,
    Color::ORANGE,
];

/// What the LED should show over time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    Off,
    Solid(Color),
    /// On for half of `period_ms`, off for the other half.
    Blink {
        color: Color,
        period_ms: u32,
    },
    /// Smooth triangle fade over `period_ms`.
    Breathe {
        color: Color,
        period_ms: u32,
    },
}

impl Pattern {
    /// Colour identifying script `index`: one palette entry per slot, and the same
    /// palette blinking for slots 8..15.
    pub fn for_script(index: usize) -> Self {
        let color = PALETTE[index % PALETTE.len()];
        if index < PALETTE.len() {
            Pattern::Solid(color)
        } else {
            Pattern::Blink {
                color,
                period_ms: 500,
            }
        }
    }

    fn color_at(self, now_ms: u64) -> Color {
        match self {
            Pattern::Off => Color::OFF,
            Pattern::Solid(color) => color,
            Pattern::Blink { color, period_ms } => {
                let phase = now_ms % period_ms as u64;
                if phase < period_ms as u64 / 2 {
                    color
                } else {
                    Color::OFF
                }
            }
            Pattern::Breathe { color, period_ms } => {
                let half = (period_ms / 2).max(1) as u64;
                let phase = now_ms % (2 * half);
                let level = if phase < half {
                    phase
                } else {
                    2 * half - phase
                };
                // Keep a faint glow at the bottom so the LED never fully disappears.
                color.scaled((level * 230 / half + 25) as u32, 255)
            }
        }
    }
}

/// A GPIO handed over to the PWM block.
type PwmPin<G> = Pin<G, FunctionPwm, PullDown>;

/// A short colour override on top of the current pattern.
#[derive(Debug, Clone, Copy)]
struct Flash {
    color: Color,
    until_ms: u64,
}

/// The status LED.
pub struct RgbLed {
    pwm1: Slice<Pwm1, FreeRunning>,
    pwm2: Slice<Pwm2, FreeRunning>,
    _pins: (PwmPin<Gpio2>, PwmPin<Gpio3>, PwmPin<Gpio4>),
    pattern: Pattern,
    flash: Option<Flash>,
    current: Option<Color>,
}

/// PWM resolution: 8 bits per channel.
const PWM_TOP: u16 = 255;
/// 125 MHz / 20 / 256 gives a flicker-free 24 kHz PWM.
const PWM_DIV: u8 = 20;

impl RgbLed {
    pub fn new(
        mut pwm1: Slice<Pwm1, FreeRunning>,
        mut pwm2: Slice<Pwm2, FreeRunning>,
        red: Pin<Gpio3, FunctionNull, PullDown>,
        green: Pin<Gpio4, FunctionNull, PullDown>,
        blue: Pin<Gpio2, FunctionNull, PullDown>,
    ) -> Self {
        for slice_setup in [&mut pwm1 as &mut dyn SliceSetup, &mut pwm2] {
            slice_setup.configure();
        }
        // Common anode: invert the outputs so a duty of 0 leaves the LED dark.
        let blue = pwm1.channel_a.output_to(blue);
        pwm1.channel_a.set_inverted();
        let red = pwm1.channel_b.output_to(red);
        pwm1.channel_b.set_inverted();
        let green = pwm2.channel_a.output_to(green);
        pwm2.channel_a.set_inverted();

        let mut led = Self {
            pwm1,
            pwm2,
            _pins: (blue, red, green),
            pattern: Pattern::Off,
            flash: None,
            current: None,
        };
        led.write(Color::OFF);
        led
    }

    /// Replaces the steady-state pattern.
    pub fn set_pattern(&mut self, pattern: Pattern) {
        self.pattern = pattern;
    }

    /// Shows `color` for `duration_ms`, then returns to the pattern.
    pub fn flash(&mut self, color: Color, duration_ms: u32, now_ms: u64) {
        self.flash = Some(Flash {
            color,
            until_ms: now_ms + duration_ms as u64,
        });
    }

    /// Updates the outputs for the current time. Call it from the main loop.
    pub fn tick(&mut self, now_ms: u64) {
        let color = match self.flash {
            Some(flash) if now_ms < flash.until_ms => flash.color,
            Some(_) => {
                self.flash = None;
                self.pattern.color_at(now_ms)
            }
            None => self.pattern.color_at(now_ms),
        };
        if self.current != Some(color) {
            self.write(color);
        }
    }

    fn write(&mut self, color: Color) {
        let _ = self.pwm1.channel_b.set_duty_cycle(gamma(color.r));
        let _ = self.pwm2.channel_a.set_duty_cycle(gamma(color.g));
        let _ = self.pwm1.channel_a.set_duty_cycle(gamma(color.b));
        self.current = Some(color);
    }
}

/// Perceptual brightness correction (square law) mapped onto the PWM range.
fn gamma(value: u8) -> u16 {
    let v = value as u32;
    ((v * v + 127) / 255) as u16
}

/// Common configuration for both PWM slices, independent of the slice type.
trait SliceSetup {
    fn configure(&mut self);
}

impl<I: rp2040_hal::pwm::SliceId> SliceSetup for Slice<I, FreeRunning> {
    fn configure(&mut self) {
        self.default_config();
        self.set_top(PWM_TOP);
        self.set_div_int(PWM_DIV);
        self.enable();
    }
}
