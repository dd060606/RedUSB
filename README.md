# RedUSB

RedUSB is a DIY USB macro device: a custom PCB built around an **Adafruit KB2040 (RP2040)** that acts as a programmable USB HID keyboard. You write simple scripts in a Ducky-Script–style language, drop them onto the device like a USB stick, and play them back as real keystrokes.

A single physical switch flips the device between two personalities:

- **Config mode** : the device appears as a USB flash drive. Copy your `.txt` scripts onto it.
- **Attack mode** : the device appears as a USB keyboard. The selected script runs automatically on connect (or on a button press).

The firmware is written in Rust (`no_std`) and runs bare-metal on the RP2040.

---

## Features

- **Ducky-Script–style language** : `STRING`, `DELAY`, `REPEAT`, key combos (`GUI r`, `CTRL ALT t`, ...), and more.
- **Up to 16 scripts on the device**, selectable with the button; an RGB LED shows which one is active.
- **Auto-run on connect** : in attack mode the selected script fires once the host is ready.
- **US / French (AZERTY) keyboard layouts**, switchable per script with `LAYOUT`.
- **Conditional execution** per host OS with `IF_OS` / `ELSE` / `END_IF` (best-effort detection).
- **Firmware updates over USB** (UF2), no external programmer required.

---

## How it works

The **mode switch** (GPIO8) selects the personality at power-up:

| Switch       | Mode       | USB device                    | Button does               |
| ------------ | ---------- | ----------------------------- | ------------------------- |
| To GND (low) | **Config** | Mass storage (`REDUSB` drive) | Cycle the selected script |
| Open (high)  | **Attack** | HID keyboard                  | Run the selected script   |

Flipping the switch while plugged in reboots the device into the other mode.

### Config mode

The device mounts as a FAT drive named `REDUSB`. Copy script files (`.txt`) onto it. Each button press selects the next script, and the LED shows its slot color so you know which one is armed. The selection is saved to flash and survives power cycles.

### Attack mode

The device enumerates as a USB keyboard. About a second after the host finishes setting it up, the **selected script runs automatically**. You can also press the button at any time to re-run it.

> **Safety:** because the script auto-runs on connect, make sure the right script is selected and the intended window is focused before you plug into attack mode. Only use this on machines you are authorized to test.

---

## Hardware

- **MCU board:** Adafruit KB2040 (RP2040, 8 MB QSPI flash)
- **Push button** on GPIO5 (active-low, to GND)
- **Mode switch** on GPIO8 (active-low, to GND)
- **RGB LED** (common anode, active-low): blue = GPIO2, red = GPIO3, green = GPIO4
- **USB-A** connector and a 500 mA resettable fuse on the PCB

KiCad sources, the 3D model, and production/assembly files live in `PCB/` and `3D/`.

---

## Repository layout

```
RedUSB/
├── 3D/            Mechanical / enclosure model
├── PCB/           KiCad project and fabrication files
├── Firmware/      Rust firmware (Cargo workspace)
│   ├── core/      Hardware-independent logic: FAT, Ducky engine, layouts (host-tested)
│   └── firmware/  RP2040 binary: USB, flash storage, LED, button, modes
├── scripts/       Ready-to-run test / example scripts
└── README.md
```

---

## Building and flashing

### Prerequisites

Install the [Rust toolchain](https://rustup.rs/) (the correct target is pinned by `rust-toolchain.toml`), then the two build/flash helpers:

```bash
cargo install flip-link      # linker used by the firmware
cargo install elf2uf2-rs     # builds the UF2 and copies it to the board
```

All firmware commands below are run from the `Firmware/firmware/` directory.

### Flashing

`cargo run --release` builds **and** flashes in a single step : the project's Cargo runner converts the firmware to a `.uf2` and copies it to the board, so there's no separate build command to remember.

1. Put the board into bootloader mode. It should appear as the `RPI-RP2` drive:
    - **First flash (blank board):** hold the **BOOT button on the KB2040 itself** while plugging in the USB cable.
    - **Updating an already-flashed board:** set the mode switch to **Config**, hold the **PCB button (GPIO5)**, then plug in : the firmware reboots straight into the bootloader.
2. Flash it:
    ```bash
    cargo run --release
    ```
    The board reboots into the new firmware when it finishes.

> To just compile without flashing (for example to check it builds), use `cargo build --release`.

---

## Writing and running scripts

Scripts are plain `.txt` files placed in the root of the `REDUSB` drive (config mode). Filenames follow the FAT **8.3** convention (up to 8 characters before the extension), and scripts are sorted by name, so prefixes like `1_`, `2_` give a predictable order. Up to 16 scripts are tracked.

See the [`scripts/`](scripts/) folder for ready-to-run examples that exercise every feature.

### Workflow

1. Set the switch to **Config** and plug in. The `REDUSB` drive appears.
2. Copy your `.txt` script(s) onto it.
3. Press the button to select a script (the LED color tells you which : see below).
4. Focus the target window, then set the switch to **Attack** and plug in. The selected script runs automatically (or press the button).

### Script language

| Command                     | Example                              | Meaning                                                        |
| --------------------------- | ------------------------------------ | -------------------------------------------------------------- |
| `REM`                       | `REM a comment`                      | Comment, ignored                                               |
| `STRING`                    | `STRING hello`                       | Type text (no newline)                                         |
| `STRINGLN`                  | `STRINGLN hello`                     | Type text, then Enter                                          |
| `DELAY`                     | `DELAY 500`                          | Wait 500 ms                                                    |
| `DEFAULT_DELAY`             | `DEFAULT_DELAY 50`                   | Pause after every command                                      |
| `REPEAT`                    | `REPEAT 3`                           | Repeat the previous command 3 more times                       |
| `LAYOUT`                    | `LAYOUT FR`                          | Keyboard layout for `STRING` (`US` or `FR`)                    |
| Key / combo                 | `GUI r`, `CTRL ALT t`, `ENTER`, `F5` | Press a key, optionally with modifiers                         |
| `IF_OS` … `ELSE` … `END_IF` | `IF_OS WINDOWS`                      | Run a block only on a given OS                                 |
| `OS_REPORT`                 | `OS_REPORT`                          | Type the detected OS and USB enumeration counters (diagnostic) |

Modifiers: `CTRL`, `SHIFT`, `ALT`, `GUI`/`WINDOWS`, `ALTGR`. The default layout is French (AZERTY).

---

## LED reference

**Config mode : selected script slot:**

| Slot  | 0   | 1     | 2    | 3      | 4    | 5       | 6     | 7      |
| ----- | --- | ----- | ---- | ------ | ---- | ------- | ----- | ------ |
| Color | Red | Green | Blue | Yellow | Cyan | Magenta | White | Orange |

(Slots 8–15 use the same colors, blinking. A slow white "breathing" glow means no scripts are present.)

**Attack mode:**

| LED                     | Meaning                                        |
| ----------------------- | ---------------------------------------------- |
| Dim selected-slot color | Idle, ready                                    |
| Solid white             | Script running                                 |
| Red flash (1.5 s)       | No script to run                               |
| Magenta flash           | Keystroke timed out (host not accepting input) |
| Orange flash            | Script line could not be parsed                |
| Yellow flash            | Script file could not be read from flash       |
| Cyan flash              | Malformed `IF_OS` / `ELSE` / `END_IF`          |

---

## Debugging

For everyday use the LED is enough. For detailed logs, the firmware emits [`defmt`](https://defmt.ferrous-systems.com/) over RTT, which needs an SWD debug probe (for example a second Raspberry Pi Pico flashed as a debug probe) wired to the KB2040's SWD pads:

```bash
cargo install probe-rs-tools
probe-rs attach --chip RP2040 target/thumbv6m-none-eabi/release/firmware
```

Host-side logic in `core/` is unit-tested:

```bash
cd Firmware/core && cargo test
```

---

## License

MIT © dd060606
