# RedUSB firmware

Rust firmware for the RedUSB macro key (Adafruit KB2040, RP2040).

The device has two personalities, selected by the switch on D8:

| Switch | Mode          | USB device         | Push button (D5)                                                                            |
| ------ | ------------- | ------------------ | ------------------------------------------------------------------------------------------- |
| closed | Configuration | Mass-storage drive | Short press: next script. Held while plugging in: reboot to the UF2 bootloader (`RPI-RP2`). |
| open   | Normal        | HID keyboard       | Runs the selected script.                                                                   |

Scripts are plain `.txt` files in the root of the `REDUSB` drive, written in a
Ducky Script compatible syntax. Flipping the switch while plugged in reboots the
device into the other mode.

## Repository layout

```
Firmware/
├── core/       redusb-core: no_std logic, tested on the host (FAT12/16, parser, interpreter, layouts)
├── firmware/   redusb-firmware: the RP2040 binary (board, LED, button, flash, USB classes, superloops)
└── scripts/    example macros to copy on the drive
```

## Building and flashing

Prerequisites (once):

```
rustup target add thumbv6m-none-eabi
cargo install flip-link elf2uf2-rs
```

Run the host tests:

```
cargo test -p redusb-core
```

Build and flash over USB (no debug probe needed):

1. Put the board in bootloader mode: either hold the KB2040 `BOOT` button while
   plugging it in, or, with the firmware already installed, set the switch to
   configuration mode and hold the RedUSB push button while plugging in.
2. From `firmware/`, run `cargo run --release`. `elf2uf2-rs -d` converts the ELF
   and copies it to the `RPI-RP2` drive.

To produce a UF2 file instead:

```
cd firmware
cargo build --release
elf2uf2-rs ../target/thumbv6m-none-eabi/release/redusb-firmware redusb.uf2
```

`defmt` logging is compiled in. It is only visible with an SWD probe attached
(`probe-rs run --chip RP2040 ...`); without one it costs nothing.

## Flash layout (8 MiB)

| Offset     | Size     | Use                                         |
| ---------- | -------- | ------------------------------------------- |
| `0x000000` | 1 MiB    | Firmware                                    |
| `0x100000` | ~6.9 MiB | FAT12 volume `REDUSB`, exposed over USB     |
| `0x7FF000` | 4 KiB    | Settings (selected script), append-only log |

The volume is formatted by the firmware the first time it boots in configuration
mode. Reformatting it from the host (FAT, any cluster size) is supported.

## Script syntax

One command per line. Commands are case-insensitive, files may be UTF-8 with or
without BOM, line endings may be LF or CRLF. Files are listed in alphabetical
order of their 8.3 name, so prefix them with numbers (`01_...`, `02_...`).

| Line                      | Effect                                                                                  |
| ------------------------- | --------------------------------------------------------------------------------------- |
| `REM text`                | Comment.                                                                                |
| `STRING text`             | Types `text`. Characters the layout cannot produce are skipped.                         |
| `STRINGLN text`           | Types `text` then presses ENTER.                                                        |
| `DELAY ms`                | Waits.                                                                                  |
| `DEFAULT_DELAY ms`        | Extra wait after every following command (`DEFAULTDELAY` too).                          |
| `REPEAT n`                | Repeats the previous command `n` more times.                                            |
| `LAYOUT US` / `LAYOUT FR` | Host layout used by `STRING` (default: US, see `app/mod.rs`).                           |
| `IF_OS <os>`              | Start a block that runs only on `<os>`: `WINDOWS`, `MAC`, `LINUX`, or `OTHER`.          |
| `ELSE`                    | Alternative branch of the enclosing `IF_OS`.                                            |
| `END_IF`                  | Ends the enclosing `IF_OS` block (`ENDIF` too).                                         |
| key combo                 | Modifiers then at most one key: `CTRL ALT t`, `GUI r`, `CTRL-SHIFT ESC`, `ENTER`, `F5`. |

Modifiers: `CTRL`/`CONTROL`, `SHIFT`, `ALT`, `GUI`/`WINDOWS`/`COMMAND`, `ALTGR`.

Keys: `ENTER`, `ESC`, `TAB`, `SPACE`, `BACKSPACE`, `DELETE`, `INSERT`, `HOME`,
`END`, `PAGEUP`, `PAGEDOWN`, `UP`/`UPARROW`, `DOWN`, `LEFT`, `RIGHT`, `CAPSLOCK`,
`NUMLOCK`, `SCROLLLOCK`, `PRINTSCREEN`, `PAUSE`, `MENU`, `F1`..`F12`, and any single
letter or digit (`CTRL c`).

Limits: 512 bytes per line, 16 scripts, root directory only, `IF_OS` nested up to 8
deep.

### Host OS detection

`IF_OS` runs a block only on a given operating system. The device cannot ask the
host what it is, so it guesses from the pattern of USB descriptor requests the host
makes while enumerating the keyboard:

- **Windows** is detected reliably: it is the only OS that probes for the Microsoft
  OS string descriptor (string index `0xEE`).
- **Linux vs macOS** is a best-effort guess from how many times the host reads the
  configuration descriptor. Calibrate the threshold in `core/src/os.rs`
  (`LINUX_MIN_CONFIG_REQUESTS`) against your own machines; the firmware logs the raw
  counters over defmt each time a script runs.
- An unrecognised host stays `unknown`; write an `ELSE` (or `IF_OS OTHER`) branch to
  cover it.

```
IF_OS WINDOWS
  GUI r
  STRINGLN cmd
ELSE
  CTRL ALT t
END_IF
```

See `scripts/03_os_aware.txt` for a fuller example.

## LED legend

| Mode          | LED                      | Meaning                         |
| ------------- | ------------------------ | ------------------------------- |
| Configuration | palette colour           | Selected script (see below)     |
| Configuration | palette colour, blinking | Selected script number 9 to 16  |
| Configuration | white, breathing         | No script on the drive          |
| Configuration | brief blank              | Host is writing to the flash    |
| Normal        | dim palette colour       | Idle, script selected           |
| Normal        | white                    | Script running                  |
| Normal        | red flash                | Script error, or nothing to run |
| Normal        | faint red, breathing     | No script on the drive          |

Palette by script position: red, green, blue, yellow, cyan, magenta, white, orange.
