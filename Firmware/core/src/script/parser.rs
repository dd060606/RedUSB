//! Line parser for the Ducky Script dialect understood by the device.
//!
//! Supported lines:
//! * `REM text` and blank lines: ignored.
//! * `STRING text` / `STRINGLN text`: type text, the latter followed by ENTER.
//! * `DELAY ms`, `DEFAULT_DELAY ms` (also `DEFAULTDELAY`).
//! * `REPEAT n`: run the previous command again `n` times.
//! * `LAYOUT US|FR`: layout used by `STRING`, defaults to the firmware setting.
//! * Key combos: modifiers followed by at most one key, e.g. `CTRL ALT t`, `GUI r`,
//!   `CTRL-SHIFT ESC`, `ENTER`, `F5`, `DOWNARROW`.

use super::keys::{KeyStroke, key_by_name, modifier_by_name};
use super::layout::Layout;

/// One parsed script line. Text payloads borrow from the input line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command<'a> {
    /// Blank line or comment.
    Empty,
    String(&'a str),
    StringLn(&'a str),
    Delay(u32),
    DefaultDelay(u32),
    Repeat(u32),
    Layout(Layout),
    Keys(KeyStroke),
}

/// Why a line could not be parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    UnknownCommand,
    MissingArgument,
    InvalidNumber,
    UnknownKey,
    TooManyKeys,
    UnknownLayout,
}

/// Splits `line` at the first blank: the command word and everything after that
/// single separator (so `STRING  two spaces` keeps its leading space).
fn split_first_word(line: &str) -> (&str, &str) {
    match line.find([' ', '\t']) {
        Some(i) => (&line[..i], &line[i + 1..]),
        None => (line, ""),
    }
}

fn parse_number(arg: &str) -> Result<u32, ParseError> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Err(ParseError::MissingArgument);
    }
    arg.parse::<u32>().map_err(|_| ParseError::InvalidNumber)
}

fn parse_keys(line: &str) -> Result<KeyStroke, ParseError> {
    let mut stroke = KeyStroke::default();
    let mut has_key = false;
    let mut first = true;
    // Ducky allows both `CTRL ALT t` and `CTRL-ALT t`.
    let tokens = line
        .split_whitespace()
        .flat_map(|t| t.split('-').filter(|p| !p.is_empty()));
    for token in tokens {
        if let Some(m) = modifier_by_name(token) {
            stroke.modifiers |= m;
        } else if let Some(u) = key_by_name(token) {
            if has_key {
                return Err(ParseError::TooManyKeys);
            }
            stroke.usage = u;
            has_key = true;
        } else if first {
            return Err(ParseError::UnknownCommand);
        } else {
            return Err(ParseError::UnknownKey);
        }
        first = false;
    }
    Ok(stroke)
}

/// Parses one line. Trailing CR/LF and leading indentation are ignored.
pub fn parse_line(line: &str) -> Result<Command<'_>, ParseError> {
    let line = line.trim_end_matches(['\r', '\n']).trim_start();
    if line.is_empty() {
        return Ok(Command::Empty);
    }
    let (head, rest) = split_first_word(line);
    let cmd = if head.eq_ignore_ascii_case("REM") {
        Command::Empty
    } else if head.eq_ignore_ascii_case("STRING") {
        Command::String(rest)
    } else if head.eq_ignore_ascii_case("STRINGLN") {
        Command::StringLn(rest)
    } else if head.eq_ignore_ascii_case("DELAY") {
        Command::Delay(parse_number(rest)?)
    } else if head.eq_ignore_ascii_case("DEFAULT_DELAY")
        || head.eq_ignore_ascii_case("DEFAULTDELAY")
    {
        Command::DefaultDelay(parse_number(rest)?)
    } else if head.eq_ignore_ascii_case("REPEAT") {
        Command::Repeat(parse_number(rest)?)
    } else if head.eq_ignore_ascii_case("LAYOUT") {
        let name = rest.trim();
        if name.is_empty() {
            return Err(ParseError::MissingArgument);
        }
        Command::Layout(Layout::from_name(name).ok_or(ParseError::UnknownLayout)?)
    } else {
        Command::Keys(parse_keys(line)?)
    };
    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::keys::{modifier, usage};

    #[test]
    fn comments_and_blank_lines() {
        assert_eq!(parse_line(""), Ok(Command::Empty));
        assert_eq!(parse_line("   \r"), Ok(Command::Empty));
        assert_eq!(parse_line("REM hello world"), Ok(Command::Empty));
        assert_eq!(parse_line("rem lower"), Ok(Command::Empty));
    }

    #[test]
    fn strings_keep_payload_verbatim() {
        assert_eq!(
            parse_line("STRING hello world"),
            Ok(Command::String("hello world"))
        );
        assert_eq!(parse_line("STRING  two"), Ok(Command::String(" two")));
        assert_eq!(
            parse_line("STRING trailing \r\n"),
            Ok(Command::String("trailing "))
        );
        assert_eq!(parse_line("STRINGLN cmd"), Ok(Command::StringLn("cmd")));
        assert_eq!(parse_line("STRING"), Ok(Command::String("")));
        assert_eq!(
            parse_line("  string indented"),
            Ok(Command::String("indented"))
        );
    }

    #[test]
    fn numeric_commands() {
        assert_eq!(parse_line("DELAY 500"), Ok(Command::Delay(500)));
        assert_eq!(
            parse_line("DEFAULT_DELAY 20"),
            Ok(Command::DefaultDelay(20))
        );
        assert_eq!(parse_line("DEFAULTDELAY 20"), Ok(Command::DefaultDelay(20)));
        assert_eq!(parse_line("REPEAT 3"), Ok(Command::Repeat(3)));
        assert_eq!(parse_line("DELAY"), Err(ParseError::MissingArgument));
        assert_eq!(parse_line("DELAY abc"), Err(ParseError::InvalidNumber));
        assert_eq!(parse_line("DELAY -1"), Err(ParseError::InvalidNumber));
    }

    #[test]
    fn layout_command() {
        assert_eq!(parse_line("LAYOUT FR"), Ok(Command::Layout(Layout::Fr)));
        assert_eq!(parse_line("LAYOUT us"), Ok(Command::Layout(Layout::Us)));
        assert_eq!(parse_line("LAYOUT"), Err(ParseError::MissingArgument));
        assert_eq!(parse_line("LAYOUT DE"), Err(ParseError::UnknownLayout));
    }

    #[test]
    fn key_combos() {
        assert_eq!(
            parse_line("ENTER"),
            Ok(Command::Keys(KeyStroke::key(usage::ENTER)))
        );
        assert_eq!(
            parse_line("GUI r"),
            Ok(Command::Keys(KeyStroke::new(usage::R, modifier::LEFT_GUI)))
        );
        assert_eq!(
            parse_line("CTRL ALT t"),
            Ok(Command::Keys(KeyStroke::new(
                usage::T,
                modifier::LEFT_CTRL | modifier::LEFT_ALT
            )))
        );
        assert_eq!(
            parse_line("CTRL-SHIFT ESC"),
            Ok(Command::Keys(KeyStroke::new(
                usage::ESCAPE,
                modifier::LEFT_CTRL | modifier::LEFT_SHIFT
            )))
        );
        assert_eq!(
            parse_line("CTRL"),
            Ok(Command::Keys(KeyStroke::new(
                usage::NONE,
                modifier::LEFT_CTRL
            )))
        );
        assert_eq!(
            parse_line("F5"),
            Ok(Command::Keys(KeyStroke::key(usage::F1 + 4)))
        );
    }

    #[test]
    fn key_errors() {
        assert_eq!(parse_line("FROBNICATE"), Err(ParseError::UnknownCommand));
        assert_eq!(parse_line("CTRL FROB"), Err(ParseError::UnknownKey));
        assert_eq!(parse_line("a b"), Err(ParseError::TooManyKeys));
    }
}
