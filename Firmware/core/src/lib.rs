//! Hardware-independent, `no_std`, allocation-free core of the RedUSB firmware:
//! FAT12/16 access, the Ducky Script engine, keyboard layouts and the catalog.

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
