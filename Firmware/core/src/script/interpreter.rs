//! Executes parsed script lines against an abstract keyboard, including `IF_OS` /
//! `ELSE` / `END_IF` blocks gated on the host OS set via [`Interpreter::set_host_os`].

use super::keys::{KeyStroke, usage};
use super::layout::Layout;
use super::lines::{ByteSource, LineError, LineReader};
use super::parser::{Command, ParseError, parse_line};
use crate::os::Os;
use heapless::Vec;

/// Maximum length of one script line in bytes.
pub const MAX_LINE_LEN: usize = 512;

/// Maximum nesting depth of `IF_OS` blocks.
pub const MAX_IF_DEPTH: usize = 8;

/// Failure reported by a [`KeyboardSink`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SinkError {
    /// The host stopped accepting reports.
    Timeout,
}

/// A malformed `IF_OS` / `ELSE` / `END_IF` structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    /// `ELSE` with no open `IF_OS`, or a second `ELSE` in one block.
    UnexpectedElse,
    /// `END_IF` with no open `IF_OS`.
    UnexpectedEndIf,
    /// An `IF_OS` block was never closed.
    UnterminatedIf,
    /// `IF_OS` blocks nested deeper than [`MAX_IF_DEPTH`].
    TooDeep,
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
    Control(ControlError),
}

/// A run failure with the 1-based line it happened on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunError<E> {
    pub line: u32,
    pub kind: RunErrorKind<E>,
}

/// One open `IF_OS` block on the control-flow stack.
#[derive(Debug, Clone, Copy)]
struct IfFrame {
    /// Whether the branch currently being read should execute.
    branch_active: bool,
    /// Whether any branch of this block has run (so `ELSE` knows to stay silent).
    any_taken: bool,
    /// Whether the `ELSE` of this block has already been seen.
    else_seen: bool,
    /// Line of the opening `IF_OS`, for error reporting.
    open_line: u32,
}

/// Script executor. Reusable across runs; each run resets layout and delays but
/// keeps the host OS set by the firmware.
pub struct Interpreter {
    default_layout: Layout,
    layout: Layout,
    default_delay: u32,
    host_os: Os,
    last_line: Vec<u8, MAX_LINE_LEN>,
    frames: Vec<IfFrame, MAX_IF_DEPTH>,
}

impl Interpreter {
    pub fn new(default_layout: Layout) -> Self {
        Self {
            default_layout,
            layout: default_layout,
            default_delay: 0,
            host_os: Os::Unknown,
            last_line: Vec::new(),
            frames: Vec::new(),
        }
    }

    /// Sets the host OS used by `IF_OS`. The firmware calls this before each run.
    pub fn set_host_os(&mut self, os: Os) {
        self.host_os = os;
    }

    /// The host OS currently assumed by `IF_OS`.
    pub fn host_os(&self) -> Os {
        self.host_os
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
        self.frames.clear();

        let mut reader = LineReader::<MAX_LINE_LEN>::new();
        let mut line_no = 0u32;
        loop {
            let line = match reader.next_line(src) {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(e) => {
                    return Err(RunError {
                        line: line_no + 1,
                        kind: RunErrorKind::Line(e),
                    });
                }
            };
            line_no += 1;
            let cmd = parse_line(line).map_err(|e| RunError {
                line: line_no,
                kind: RunErrorKind::Parse(e),
            })?;
            match cmd {
                Command::IfOs(os) => self.enter_if(os, line_no).map_err(|e| RunError {
                    line: line_no,
                    kind: RunErrorKind::Control(e),
                })?,
                Command::Else => self.enter_else().map_err(|e| RunError {
                    line: line_no,
                    kind: RunErrorKind::Control(e),
                })?,
                Command::EndIf => self.end_if().map_err(|e| RunError {
                    line: line_no,
                    kind: RunErrorKind::Control(e),
                })?,
                Command::Repeat(count) => {
                    if self.executing() {
                        // Re-parse the saved copy so the repeated command can borrow it.
                        let saved = self.last_line.clone();
                        let text = core::str::from_utf8(&saved).unwrap_or("");
                        let prev = parse_line(text).map_err(|e| RunError {
                            line: line_no,
                            kind: RunErrorKind::Parse(e),
                        })?;
                        for _ in 0..count {
                            self.execute(prev, host).map_err(|e| RunError {
                                line: line_no,
                                kind: RunErrorKind::Sink(e),
                            })?;
                        }
                    }
                }
                other => {
                    if self.executing() {
                        let executed = self.execute(other, host).map_err(|e| RunError {
                            line: line_no,
                            kind: RunErrorKind::Sink(e),
                        })?;
                        if executed {
                            self.last_line.clear();
                            let _ = self.last_line.extend_from_slice(line.as_bytes());
                        }
                    }
                }
            }
        }
        if let Some(frame) = self.frames.first() {
            return Err(RunError {
                line: frame.open_line,
                kind: RunErrorKind::Control(ControlError::UnterminatedIf),
            });
        }
        Ok(())
    }

    /// Whether the current line should run: every open block is on an active branch.
    fn executing(&self) -> bool {
        self.frames.iter().all(|f| f.branch_active)
    }

    fn enter_if(&mut self, os: Os, line: u32) -> Result<(), ControlError> {
        let parent_active = self.executing();
        let active = parent_active && os == self.host_os;
        self.frames
            .push(IfFrame {
                branch_active: active,
                any_taken: active,
                else_seen: false,
                open_line: line,
            })
            .map_err(|_| ControlError::TooDeep)
    }

    fn enter_else(&mut self) -> Result<(), ControlError> {
        let depth = self.frames.len();
        if depth == 0 {
            return Err(ControlError::UnexpectedElse);
        }
        // The block runs only if its parent runs and no earlier branch was taken.
        let parent_active = self.frames[..depth - 1].iter().all(|f| f.branch_active);
        let frame = &mut self.frames[depth - 1];
        if frame.else_seen {
            return Err(ControlError::UnexpectedElse);
        }
        frame.else_seen = true;
        let active = parent_active && !frame.any_taken;
        frame.branch_active = active;
        frame.any_taken |= active;
        Ok(())
    }

    fn end_if(&mut self) -> Result<(), ControlError> {
        self.frames
            .pop()
            .map(|_| ())
            .ok_or(ControlError::UnexpectedEndIf)
    }

    /// Executes one command. Returns whether it was a repeatable action.
    fn execute<H>(&mut self, cmd: Command<'_>, host: &mut H) -> Result<bool, SinkError>
    where
        H: KeyboardSink + Delay,
    {
        match cmd {
            // Structural and no-op commands are handled by `run` or do nothing here.
            Command::Empty
            | Command::Repeat(_)
            | Command::IfOs(_)
            | Command::Else
            | Command::EndIf => return Ok(false),
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
        run_on(script, Os::Unknown)
    }

    fn run_on(script: &str, os: Os) -> Result<StdVec<Ev>, RunError<core::convert::Infallible>> {
        let mut host = Recorder::default();
        let mut src = script.as_bytes();
        let mut interp = Interpreter::new(Layout::Us);
        interp.set_host_os(os);
        interp.run(&mut src, &mut host)?;
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
        let mut interp = Interpreter::new(Layout::Us);
        let err = interp.run(&mut src, &mut host).unwrap_err();
        assert_eq!(err.kind, RunErrorKind::Sink(SinkError::Timeout));
        assert_eq!(host.events.len(), 2);
    }

    const OS_SCRIPT: &str = "IF_OS WINDOWS\nSTRING w\nELSE\nSTRING o\nEND_IF\nENTER";

    #[test]
    fn if_os_runs_matching_branch() {
        let ev = run_on(OS_SCRIPT, Os::Windows).unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::W, 0));
        expected.extend(tap(usage::ENTER, 0));
        assert_eq!(ev, expected);
    }

    #[test]
    fn if_os_runs_else_branch() {
        let ev = run_on(OS_SCRIPT, Os::Linux).unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::O, 0));
        expected.extend(tap(usage::ENTER, 0));
        assert_eq!(ev, expected);
    }

    #[test]
    fn unknown_os_takes_else() {
        let ev = run_on(OS_SCRIPT, Os::Unknown).unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::O, 0));
        expected.extend(tap(usage::ENTER, 0));
        assert_eq!(ev, expected);
    }

    #[test]
    fn if_os_without_else_skips_when_no_match() {
        let ev = run_on("IF_OS MAC\nSTRING x\nEND_IF\nENTER", Os::Windows).unwrap();
        assert_eq!(ev, tap(usage::ENTER, 0));
    }

    #[test]
    fn nested_conditionals() {
        let script =
            "IF_OS LINUX\nSTRING a\nIF_OS WINDOWS\nSTRING b\nELSE\nSTRING c\nEND_IF\nEND_IF";
        let ev = run_on(script, Os::Linux).unwrap();
        let mut expected = StdVec::new();
        expected.extend(tap(usage::A, 0));
        expected.extend(tap(usage::C, 0));
        assert_eq!(ev, expected);
        // The whole outer block is skipped, inner structure included, on a mismatch.
        assert!(run_on(script, Os::MacOs).unwrap().is_empty());
    }

    #[test]
    fn repeat_inside_skipped_branch_is_ignored() {
        let ev = run_on("IF_OS WINDOWS\nSTRING a\nREPEAT 5\nEND_IF", Os::Linux).unwrap();
        assert!(ev.is_empty());
    }

    #[test]
    fn else_without_if_errors() {
        let err = run("STRING a\nELSE").unwrap_err();
        assert_eq!(err.line, 2);
        assert_eq!(
            err.kind,
            RunErrorKind::Control(ControlError::UnexpectedElse)
        );
    }

    #[test]
    fn endif_without_if_errors() {
        let err = run("END_IF").unwrap_err();
        assert_eq!(
            err.kind,
            RunErrorKind::Control(ControlError::UnexpectedEndIf)
        );
    }

    #[test]
    fn unterminated_if_errors_at_open_line() {
        let err = run_on("ENTER\nIF_OS WINDOWS\nSTRING x", Os::Windows).unwrap_err();
        assert_eq!(err.line, 2);
        assert_eq!(
            err.kind,
            RunErrorKind::Control(ControlError::UnterminatedIf)
        );
    }

    #[test]
    fn double_else_errors() {
        let err = run_on("IF_OS WINDOWS\nELSE\nELSE\nEND_IF", Os::Windows).unwrap_err();
        assert_eq!(
            err.kind,
            RunErrorKind::Control(ControlError::UnexpectedElse)
        );
    }
}
