//! Hardware-independent logic of the RedUSB firmware.
//!
//! Everything in this crate is `no_std`, allocation-free and testable on a host
//! machine: FAT12/16 volume access, the Ducky Script parser and interpreter,
//! keyboard layouts and the script catalog.

#![no_std]
#![deny(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod block;
pub mod catalog;
pub mod fat;
pub mod os;
pub mod script;

#[cfg(test)]
pub(crate) mod testing;
