//! Executes parsed script lines against an abstract keyboard.

use super::keys::{KeyStroke, usage};
use super::layout::Layout;
use super::lines::{ByteSource, LineError, LineReader};
use super::parser::{Command, ParseError, parse_line};
use heapless::Vec;

/// Maximum length of one script line in bytes.
pub const MAX_LINE_LEN: usize = 512;

/// Failure reported by a [`KeyboardSink`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkError {
    /// The host stopped accepting reports.
    Timeout,
}

/// Where key events go. The firmware implements this with the USB HID class.
pub trait KeyboardSink {
    /// Sends a report with `stroke` held down.
    fn press(&mut self, stroke: KeyStroke) -> Result<(), SinkError>;

    /// Sends an empty report.
    fn release(&mut self) -> Result<(), SinkError>;
}

/// Blocking millisecond delay. Implementations keep servicing USB meanwhile.
pub trait Delay {
    fn delay_ms(&mut self, ms: u32);
}

/// What went wrong while running a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunErrorKind<E> {
    Parse(ParseError),
    Line(LineError<E>),
    Sink(SinkError),
}

/// A run failure with the 1-based line it happened on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunError<E> {
    pub line: u32,
    pub kind: RunErrorKind<E>,
}

/// Script executor. Reusable across runs; each run starts from the default layout.
pub struct Interpreter {
    default_layout: Layout,
    layout: Layout,
    default_delay: u32,
    last_line: Vec<u8, MAX_LINE_LEN>,
}

impl Interpreter {
    pub fn new(default_layout: Layout) -> Self {
        Self {
            default_layout,
            layout: default_layout,
            default_delay: 0,
            last_line: Vec::new(),
        }
    }

    /// Runs a whole script from `src`, sending keys and delays to `host`.
    pub fn run<S, H>(&mut self, src: &mut S, host: &mut H) -> Result<(), RunError<S::Error>>
    where
        S: ByteSource,
        H: KeyboardSink + Delay,
    {
        self.layout = self.default_layout;
        self.default_delay = 0;
        self.last_line.clear();

        let mut reader = LineReader::<MAX_LINE_LEN>::new();
        let mut line_no = 0u32;
        loop {
            let line = match reader.next_line(src) {
                Ok(Some(line)) => line,
                Ok(None) => return Ok(()),
                Err(e) => {
                    return Err(RunError {
                        line: line_no + 1,
                        kind: RunErrorKind::Line(e),
                    });
                }
            };
            line_no += 1;
            let fail = |kind| RunError {
                line: line_no,
                kind,
            };
            let cmd = parse_line(line).map_err(|e| fail(RunErrorKind::Parse(e)))?;
            match cmd {
                Command::Repeat(count) => {
                    // Re-parse the saved copy so the repeated command can borrow it.
                    let saved = self.last_line.clone();
                    let text = core::str::from_utf8(&saved).unwrap_or("");
                    let prev = parse_line(text).map_err(|e| fail(RunErrorKind::Parse(e)))?;
                    for _ in 0..count {
                        self.execute(prev, host)
                            .map_err(|e| fail(RunErrorKind::Sink(e)))?;
                    }
                }
                other => {
                    let executed = self
                        .execute(other, host)
                        .map_err(|e| fail(RunErrorKind::Sink(e)))?;
                    if executed {
                        self.last_line.clear();
                        let _ = self.last_line.extend_from_slice(line.as_bytes());
                    }
                }
            }
        }
    }

    /// Executes one command. Returns whether it was a repeatable action.
    fn execute<H>(&mut self, cmd: Command<'_>, host: &mut H) -> Result<bool, SinkError>
    where
        H: KeyboardSink + Delay,
    {
        match cmd {
            Command::Empty | Command::Repeat(_) => return Ok(false),
            Command::DefaultDelay(ms) => {
                self.default_delay = ms;
                return Ok(false);
            }
            Command::Layout(layout) => {
                self.layout = layout;
                return Ok(false);
            }
            Command::String(text) => self.type_text(text, host)?,
            Command::StringLn(text) => {
                self.type_text(text, host)?;
                tap(host, KeyStroke::key(usage::ENTER))?;
            }
            Command::Delay(ms) => host.delay_ms(ms),
            Command::Keys(stroke) => tap(host, stroke)?,
        }
        if self.default_delay > 0 {
            host.delay_ms(self.default_delay);
        }
        Ok(true)
    }

    fn type_text<H: KeyboardSink>(&self, text: &str, host: &mut H) -> Result<(), SinkError> {
        for c in text.chars() {
            // Characters the layout cannot produce are silently dropped.
            if let Some(seq) = self.layout.strokes(c) {
                for &stroke in seq.as_slice() {
                    tap(host, stroke)?;
                }
            }
        }
        Ok(())
    }
}

fn tap<H: KeyboardSink>(host: &mut H, stroke: KeyStroke) -> Result<(), SinkError> {
    host.press(stroke)?;
    host.release()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::keys::modifier;
    use std::vec::Vec as StdVec;

    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    enum Ev {
        Press(u8, u8),
        Release,
        Delay(u32),
    }

    #[derive(Default)]
    struct Recorder {
        events: StdVec<Ev>,
        fail_after: Option<usize>,
    }

    impl KeyboardSink for Recorder {
        fn press(&mut self, stroke: KeyStroke) -> Result<(), SinkError> {
            if self.fail_after == Some(self.events.len()) {
                return Err(SinkError::Timeout);
            }
            self.events.push(Ev::Press(stroke.usage, stroke.modifiers));
            Ok(())
        }

        fn release(&mut self) -> Result<(), SinkError> {
            self.events.push(Ev::Release);
            Ok(())
        }
    }

    impl Delay for Recorder {
        fn delay_ms(&mut self, ms: u32) {
            self.events.push(Ev::Delay(ms));
        }
    }

    fn run(script: &str) -> Result<StdVec<Ev>, RunError<core::convert::Infallible>> {
        let mut host = Recorder::default();
        let mut src = script.as_bytes();
        Interpreter::new(Layout::Us).run(&mut src, &mut host)?;
        Ok(host.events)
    }

    fn tap(u: u8, m: u8) -> [Ev; 2] {
        [Ev::Press(u, m), Ev::Release]
    }

    #[test]
    fn types_string_and_keys() {
        let ev = run("STRING Hi\nGUI r\n").unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::H, modifier::LEFT_SHIFT));
        expected.extend(tap(usage::I, 0));
        expected.extend(tap(usage::R, modifier::LEFT_GUI));
        assert_eq!(ev, expected);
    }

    #[test]
    fn stringln_appends_enter() {
        let ev = run("STRINGLN a").unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::A, 0));
        expected.extend(tap(usage::ENTER, 0));
        assert_eq!(ev, expected);
    }

    #[test]
    fn default_delay_follows_each_command() {
        let ev = run("DEFAULT_DELAY 10\nENTER\nDELAY 5\nREM nothing\nTAB").unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::ENTER, 0));
        expected.push(Ev::Delay(10));
        expected.push(Ev::Delay(5));
        expected.push(Ev::Delay(10));
        expected.extend(tap(usage::TAB, 0));
        expected.push(Ev::Delay(10));
        assert_eq!(ev, expected);
    }

    #[test]
    fn repeat_reruns_previous_command() {
        let ev = run("STRING ab\nREM ignored\nREPEAT 2").unwrap();
        let mut once = StdVec::new();
        once.extend(tap(usage::A, 0));
        once.extend(tap(usage::B, 0));
        let mut expected = StdVec::new();
        for _ in 0..3 {
            expected.extend(once.iter().copied());
        }
        assert_eq!(ev, expected);
    }

    #[test]
    fn layout_switch_changes_typing() {
        let ev = run("LAYOUT FR\nSTRING a\nLAYOUT US\nSTRING a").unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::Q, 0));
        expected.extend(tap(usage::A, 0));
        assert_eq!(ev, expected);
    }

    #[test]
    fn untypeable_characters_are_skipped() {
        let ev = run("STRING aé").unwrap();
        assert_eq!(ev, tap(usage::A, 0));
    }

    #[test]
    fn errors_carry_line_numbers() {
        let err = run("REM ok\nENTER\nBOGUS 1\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert_eq!(err.kind, RunErrorKind::Parse(ParseError::UnknownCommand));
    }

    #[test]
    fn sink_failure_stops_the_run() {
        let mut host = Recorder {
            fail_after: Some(2),
            ..Default::default()
        };
        let mut src = "STRING abc".as_bytes();
        let err = Interpreter::new(Layout::Us)
            .run(&mut src, &mut host)
            .unwrap_err();
        assert_eq!(err.kind, RunErrorKind::Sink(SinkError::Timeout));
        assert_eq!(host.events.len(), 2);
    }
}
