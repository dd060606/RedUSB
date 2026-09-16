I'm working on an hardware project using an adafruit kb2040 with a RP2040 microcontroller. Your goal is to develop the firmware for the device in Rust, which will allow it to function as a HID device (custom keyboard).

## Hardware Overview

On the PCB, there is a USB port, it's the main interface for communication with a host computer.
There is a push button on the D5 pin, which will be used to trigger a specific action when pressed.
There is a switch on the D8 pin, which will be used to change the device's mode between configuration mode (mass storage) and normal mode (HID).
There is a RGB LED (D4 pin for green, D3 pin for red, and D2 pin for blue) that can be used to show the current status of the device.

## Core Functionality

The firmware will implement the following core functionalities:

1. When the device is in the configuration mode, the device will present itself as a mass storage device to the host computer. In this mode, the user can upload script files to the device, and the scripts are stored in the flash memory.
2. When the device is in configuration mode, the push button can be used to rotate through the available scripts stored in the flash memory. The RGB LED will indicate the currently selected script by changing its color.
3. Auto-flash: when plugged into a host computer when in configuration mode and when the push button is pressed, the device will automatically reboot to flash so the computer can upload new firmware to the device.
4. The goal of the device is to implement advanced macro functionality for a keyboard, so uploaded scripts are interpreted by the device (a custom code interpreter (simple) is needed) to perform advanced keyboard macros.
5. When the device is in normal mode, it will function as a HID keyboard, when the push button is pressed, the current script will be executed (custom macro).

## Code style

Avoid using multi lines comments unless for docstrings, and prefer using single line comments for clarity.
Ensure the code architecture is clean, modular and professional.
