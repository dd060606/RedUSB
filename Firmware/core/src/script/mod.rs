//! Ducky Script macro engine: [`parser`] reads a line into a [`Command`], [`layout`]
//! maps characters to HID strokes, and [`interpreter`] runs a line stream.

pub mod interpreter;
pub mod keys;
pub mod layout;
pub mod lines;
pub mod parser;

pub use crate::os::Os;
pub use interpreter::{
    ControlError, Delay, Interpreter, KeyboardSink, RunError, RunErrorKind, SinkError,
};
pub use keys::KeyStroke;
pub use layout::Layout;
pub use lines::{ByteSource, LineReader};
pub use parser::{Command, ParseError};
