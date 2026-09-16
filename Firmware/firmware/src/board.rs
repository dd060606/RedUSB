//! Board support for the RedUSB PCB (Adafruit KB2040).
//!
//! Pin assignment:
//!
//! | Signal          | KB2040 | GPIO | Peripheral |
//! |-----------------|--------|------|------------|
//! | LED blue        | D2     | 2    | PWM1 A     |
//! | LED red         | D3     | 3    | PWM1 B     |
//! | LED green       | D4     | 4    | PWM2 A     |
//! | Push button     | D5     | 5    | SIO input, pull-up, active low |
//! | Mode switch     | D8     | 8    | SIO input, pull-up, low = configuration mode |
//!
//! The RGB LED is common anode, so every colour channel is active low.

use crate::button::Button;
use crate::led::RgbLed;
use crate::mode::ModeSwitch;
use embedded_hal::delay::DelayNs;
use rp2040_hal::clocks::init_clocks_and_plls;
use rp2040_hal::gpio::bank0::{Gpio5, Gpio8};
use rp2040_hal::gpio::{FunctionSio, Pin, Pins, PullUp, SioInput};
use rp2040_hal::pwm::Slices;
use rp2040_hal::usb::UsbBus;
use rp2040_hal::{Sio, Timer, Watchdog, pac};
use static_cell::StaticCell;
use usb_device::bus::UsbBusAllocator;

/// Crystal frequency of the KB2040.
pub const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

/// Time given to the pull-ups to charge any debounce capacitor on the button and
/// switch before their level is trusted (the power-up BOOTSEL gesture depends on it).
const INPUT_SETTLE_MS: u32 = 50;

pub type ButtonPin = Pin<Gpio5, FunctionSio<SioInput>, PullUp>;
pub type SwitchPin = Pin<Gpio8, FunctionSio<SioInput>, PullUp>;

/// Every initialised peripheral the application needs.
pub struct Board {
    pub led: RgbLed,
    pub button: Button<ButtonPin>,
    pub mode_switch: ModeSwitch<SwitchPin>,
    pub timer: Timer,
    pub watchdog: Watchdog,
    pub usb_bus: &'static UsbBusAllocator<UsbBus>,
}

impl Board {
    /// Takes the PAC peripherals and configures the board. Must be called once.
    pub fn take() -> Self {
        let mut pac = pac::Peripherals::take().expect("peripherals already taken");
        let mut watchdog = Watchdog::new(pac.WATCHDOG);
        let Ok(clocks) = init_clocks_and_plls(
            XOSC_CRYSTAL_FREQ,
            pac.XOSC,
            pac.CLOCKS,
            pac.PLL_SYS,
            pac.PLL_USB,
            &mut pac.RESETS,
            &mut watchdog,
        ) else {
            panic!("clock init failed");
        };

        let timer = Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);
        let sio = Sio::new(pac.SIO);
        let pins = Pins::new(
            pac.IO_BANK0,
            pac.PADS_BANK0,
            sio.gpio_bank0,
            &mut pac.RESETS,
        );

        let pwm = Slices::new(pac.PWM, &mut pac.RESETS);
        let led = RgbLed::new(pwm.pwm1, pwm.pwm2, pins.gpio3, pins.gpio4, pins.gpio2);

        let button_pin = pins.gpio5.into_pull_up_input();
        let switch_pin = pins.gpio8.into_pull_up_input();
        let mut timer = timer;
        timer.delay_ms(INPUT_SETTLE_MS);
        let now = millis(timer);
        let button = Button::new(button_pin, now);
        let mode_switch = ModeSwitch::new(switch_pin, now);

        static USB_BUS: StaticCell<UsbBusAllocator<UsbBus>> = StaticCell::new();
        let usb_bus = USB_BUS.init(UsbBusAllocator::new(UsbBus::new(
            pac.USBCTRL_REGS,
            pac.USBCTRL_DPRAM,
            clocks.usb_clock,
            true,
            &mut pac.RESETS,
        )));

        Self {
            led,
            button,
            mode_switch,
            timer,
            watchdog,
            usb_bus,
        }
    }
}

/// Milliseconds since boot.
pub fn millis(timer: Timer) -> u64 {
    timer.get_counter().ticks() / 1000
}
