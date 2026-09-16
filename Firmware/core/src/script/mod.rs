//! Ducky Script compatible macro engine.
//!
//! * [`parser`] turns one text line into a [`Command`].
//! * [`layout`] converts characters to HID key strokes for a given keyboard layout.
//! * [`interpreter`] executes a stream of lines against a [`KeyboardSink`].

pub mod interpreter;
pub mod keys;
pub mod layout;
pub mod lines;
pub mod parser;

pub use interpreter::{Delay, Interpreter, KeyboardSink, RunError, RunErrorKind, SinkError};
pub use keys::KeyStroke;
pub use layout::Layout;
pub use lines::{ByteSource, LineReader};
pub use parser::{Command, ParseError};
