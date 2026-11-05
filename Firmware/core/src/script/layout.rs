//! Character-to-keystroke translation per host keyboard layout (US, FR). The host
//! decodes scan codes by its own layout; dead-key characters expand into two strokes.

use super::keys::{KeyStroke, usage};

/// Host keyboard layout used to type `STRING` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// US QWERTY.
    #[default]
    Us,
    /// French AZERTY (Windows / Linux "fr" layout).
    Fr,
}

impl Layout {
    /// Parses the argument of a `LAYOUT` line.
    pub fn from_name(name: &str) -> Option<Self> {
        if name.eq_ignore_ascii_case("US") || name.eq_ignore_ascii_case("QWERTY") {
            Some(Layout::Us)
        } else if name.eq_ignore_ascii_case("FR") || name.eq_ignore_ascii_case("AZERTY") {
            Some(Layout::Fr)
        } else {
            None
        }
    }

    /// Key strokes producing `c` on this layout, or `None` if it cannot be typed.
    pub fn strokes(self, c: char) -> Option<StrokeSeq> {
        match self {
            Layout::Us => us(c),
            Layout::Fr => fr(c),
        }
    }
}

/// One or two key strokes producing a single character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrokeSeq {
    strokes: [KeyStroke; 2],
    len: usize,
}

impl StrokeSeq {
    const fn one(k: KeyStroke) -> Self {
        Self {
            strokes: [k, KeyStroke::key(0)],
            len: 1,
        }
    }

    const fn two(first: KeyStroke, second: KeyStroke) -> Self {
        Self {
            strokes: [first, second],
            len: 2,
        }
    }

    pub fn as_slice(&self) -> &[KeyStroke] {
        &self.strokes[..self.len]
    }
}

fn one(k: KeyStroke) -> Option<StrokeSeq> {
    Some(StrokeSeq::one(k))
}

fn two(a: KeyStroke, b: KeyStroke) -> Option<StrokeSeq> {
    Some(StrokeSeq::two(a, b))
}

/// Usage of a lower-case ASCII letter on a QWERTY layout.
fn qwerty_letter(c: char) -> u8 {
    usage::A + (c as u8 - b'a')
}

fn common(c: char) -> Option<KeyStroke> {
    match c {
        ' ' => Some(KeyStroke::key(usage::SPACE)),
        '\t' => Some(KeyStroke::key(usage::TAB)),
        '\n' => Some(KeyStroke::key(usage::ENTER)),
        _ => None,
    }
}

fn us(c: char) -> Option<StrokeSeq> {
    if let Some(k) = common(c) {
        return one(k);
    }
    let k = match c {
        'a'..='z' => KeyStroke::key(qwerty_letter(c)),
        'A'..='Z' => KeyStroke::shifted(qwerty_letter(c.to_ascii_lowercase())),
        '1'..='9' => KeyStroke::key(usage::KEY_1 + (c as u8 - b'1')),
        '0' => KeyStroke::key(usage::KEY_0),
        '!' => KeyStroke::shifted(usage::KEY_1),
        '@' => KeyStroke::shifted(usage::KEY_2),
        '#' => KeyStroke::shifted(usage::KEY_3),
        '$' => KeyStroke::shifted(usage::KEY_4),
        '%' => KeyStroke::shifted(usage::KEY_5),
        '^' => KeyStroke::shifted(usage::KEY_6),
        '&' => KeyStroke::shifted(usage::KEY_7),
        '*' => KeyStroke::shifted(usage::KEY_8),
        '(' => KeyStroke::shifted(usage::KEY_9),
        ')' => KeyStroke::shifted(usage::KEY_0),
        '-' => KeyStroke::key(usage::MINUS),
        '_' => KeyStroke::shifted(usage::MINUS),
        '=' => KeyStroke::key(usage::EQUAL),
        '+' => KeyStroke::shifted(usage::EQUAL),
        '[' => KeyStroke::key(usage::LEFT_BRACKET),
        '{' => KeyStroke::shifted(usage::LEFT_BRACKET),
        ']' => KeyStroke::key(usage::RIGHT_BRACKET),
        '}' => KeyStroke::shifted(usage::RIGHT_BRACKET),
        '\\' => KeyStroke::key(usage::BACKSLASH),
        '|' => KeyStroke::shifted(usage::BACKSLASH),
        ';' => KeyStroke::key(usage::SEMICOLON),
        ':' => KeyStroke::shifted(usage::SEMICOLON),
        '\'' => KeyStroke::key(usage::QUOTE),
        '"' => KeyStroke::shifted(usage::QUOTE),
        '`' => KeyStroke::key(usage::GRAVE),
        '~' => KeyStroke::shifted(usage::GRAVE),
        ',' => KeyStroke::key(usage::COMMA),
        '<' => KeyStroke::shifted(usage::COMMA),
        '.' => KeyStroke::key(usage::DOT),
        '>' => KeyStroke::shifted(usage::DOT),
        '/' => KeyStroke::key(usage::SLASH),
        '?' => KeyStroke::shifted(usage::SLASH),
        _ => return None,
    };
    one(k)
}

/// Usage of a lower-case ASCII letter on an AZERTY layout.
fn azerty_letter(c: char) -> u8 {
    match c {
        'a' => usage::Q,
        'q' => usage::A,
        'z' => usage::W,
        'w' => usage::Z,
        'm' => usage::SEMICOLON,
        _ => qwerty_letter(c),
    }
}

/// Dead keys of the French layout.
mod fr_dead {
    use super::*;
    pub const CIRCUMFLEX: KeyStroke = KeyStroke::key(usage::LEFT_BRACKET);
    pub const DIAERESIS: KeyStroke = KeyStroke::shifted(usage::LEFT_BRACKET);
    pub const TILDE: KeyStroke = KeyStroke::alt_gr(usage::KEY_2);
    pub const GRAVE: KeyStroke = KeyStroke::alt_gr(usage::KEY_7);
}

/// Splits an accented Latin letter into (dead key, base letter).
fn fr_decompose(c: char) -> Option<(KeyStroke, char)> {
    let (dead, base) = match c {
        'â' => (fr_dead::CIRCUMFLEX, 'a'),
        'ê' => (fr_dead::CIRCUMFLEX, 'e'),
        'î' => (fr_dead::CIRCUMFLEX, 'i'),
        'ô' => (fr_dead::CIRCUMFLEX, 'o'),
        'û' => (fr_dead::CIRCUMFLEX, 'u'),
        'Â' => (fr_dead::CIRCUMFLEX, 'A'),
        'Ê' => (fr_dead::CIRCUMFLEX, 'E'),
        'Î' => (fr_dead::CIRCUMFLEX, 'I'),
        'Ô' => (fr_dead::CIRCUMFLEX, 'O'),
        'Û' => (fr_dead::CIRCUMFLEX, 'U'),
        'ä' => (fr_dead::DIAERESIS, 'a'),
        'ë' => (fr_dead::DIAERESIS, 'e'),
        'ï' => (fr_dead::DIAERESIS, 'i'),
        'ö' => (fr_dead::DIAERESIS, 'o'),
        'ü' => (fr_dead::DIAERESIS, 'u'),
        'ÿ' => (fr_dead::DIAERESIS, 'y'),
        'Ä' => (fr_dead::DIAERESIS, 'A'),
        'Ë' => (fr_dead::DIAERESIS, 'E'),
        'Ï' => (fr_dead::DIAERESIS, 'I'),
        'Ö' => (fr_dead::DIAERESIS, 'O'),
        'Ü' => (fr_dead::DIAERESIS, 'U'),
        'ì' => (fr_dead::GRAVE, 'i'),
        'ò' => (fr_dead::GRAVE, 'o'),
        'À' => (fr_dead::GRAVE, 'A'),
        'È' => (fr_dead::GRAVE, 'E'),
        'Ù' => (fr_dead::GRAVE, 'U'),
        'ã' => (fr_dead::TILDE, 'a'),
        'ñ' => (fr_dead::TILDE, 'n'),
        'õ' => (fr_dead::TILDE, 'o'),
        'Ñ' => (fr_dead::TILDE, 'N'),
        _ => return None,
    };
    Some((dead, base))
}

fn fr(c: char) -> Option<StrokeSeq> {
    if let Some(k) = common(c) {
        return one(k);
    }
    if let Some((dead, base)) = fr_decompose(c) {
        let letter = if base.is_ascii_uppercase() {
            KeyStroke::shifted(azerty_letter(base.to_ascii_lowercase()))
        } else {
            KeyStroke::key(azerty_letter(base))
        };
        return two(dead, letter);
    }
    let space = KeyStroke::key(usage::SPACE);
    let k = match c {
        'a'..='z' => KeyStroke::key(azerty_letter(c)),
        'A'..='Z' => KeyStroke::shifted(azerty_letter(c.to_ascii_lowercase())),
        // Digits sit on the shifted layer of the top row.
        '1'..='9' => KeyStroke::shifted(usage::KEY_1 + (c as u8 - b'1')),
        '0' => KeyStroke::shifted(usage::KEY_0),
        // Unshifted top row.
        '&' => KeyStroke::key(usage::KEY_1),
        'é' => KeyStroke::key(usage::KEY_2),
        '"' => KeyStroke::key(usage::KEY_3),
        '\'' => KeyStroke::key(usage::KEY_4),
        '(' => KeyStroke::key(usage::KEY_5),
        '-' => KeyStroke::key(usage::KEY_6),
        'è' => KeyStroke::key(usage::KEY_7),
        '_' => KeyStroke::key(usage::KEY_8),
        'ç' => KeyStroke::key(usage::KEY_9),
        'à' => KeyStroke::key(usage::KEY_0),
        ')' => KeyStroke::key(usage::MINUS),
        '°' => KeyStroke::shifted(usage::MINUS),
        '=' => KeyStroke::key(usage::EQUAL),
        '+' => KeyStroke::shifted(usage::EQUAL),
        '²' => KeyStroke::key(usage::GRAVE),
        // AltGr layer of the top row.
        '~' => return two(fr_dead::TILDE, space),
        '#' => KeyStroke::alt_gr(usage::KEY_3),
        '{' => KeyStroke::alt_gr(usage::KEY_4),
        '[' => KeyStroke::alt_gr(usage::KEY_5),
        '|' => KeyStroke::alt_gr(usage::KEY_6),
        '`' => return two(fr_dead::GRAVE, space),
        '\\' => KeyStroke::alt_gr(usage::KEY_8),
        '^' => KeyStroke::alt_gr(usage::KEY_9),
        '@' => KeyStroke::alt_gr(usage::KEY_0),
        ']' => KeyStroke::alt_gr(usage::MINUS),
        '}' => KeyStroke::alt_gr(usage::EQUAL),
        '€' => KeyStroke::alt_gr(usage::E),
        '¤' => KeyStroke::alt_gr(usage::RIGHT_BRACKET),
        // Right-hand punctuation keys.
        '¨' => return two(fr_dead::DIAERESIS, space),
        '$' => KeyStroke::key(usage::RIGHT_BRACKET),
        '£' => KeyStroke::shifted(usage::RIGHT_BRACKET),
        'ù' => KeyStroke::key(usage::QUOTE),
        '%' => KeyStroke::shifted(usage::QUOTE),
        '*' => KeyStroke::key(usage::NON_US_HASH),
        'µ' => KeyStroke::shifted(usage::NON_US_HASH),
        ',' => KeyStroke::key(usage::M),
        '?' => KeyStroke::shifted(usage::M),
        ';' => KeyStroke::key(usage::COMMA),
        '.' => KeyStroke::shifted(usage::COMMA),
        ':' => KeyStroke::key(usage::DOT),
        '/' => KeyStroke::shifted(usage::DOT),
        '!' => KeyStroke::key(usage::SLASH),
        '§' => KeyStroke::shifted(usage::SLASH),
        '<' => KeyStroke::key(usage::NON_US_BACKSLASH),
        '>' => KeyStroke::shifted(usage::NON_US_BACKSLASH),
        _ => return None,
    };
    one(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single(layout: Layout, c: char) -> KeyStroke {
        let seq = layout.strokes(c).unwrap();
        assert_eq!(seq.as_slice().len(), 1, "{c:?} should be one stroke");
        seq.as_slice()[0]
    }

    #[test]
    fn us_basics() {
        assert_eq!(single(Layout::Us, 'a'), KeyStroke::key(usage::A));
        assert_eq!(single(Layout::Us, 'H'), KeyStroke::shifted(usage::H));
        assert_eq!(single(Layout::Us, '1'), KeyStroke::key(usage::KEY_1));
        assert_eq!(single(Layout::Us, '0'), KeyStroke::key(usage::KEY_0));
        assert_eq!(single(Layout::Us, '@'), KeyStroke::shifted(usage::KEY_2));
        assert_eq!(single(Layout::Us, ' '), KeyStroke::key(usage::SPACE));
        assert_eq!(single(Layout::Us, '\n'), KeyStroke::key(usage::ENTER));
        assert!(Layout::Us.strokes('é').is_none());
    }

    #[test]
    fn fr_swapped_letters_and_digits() {
        assert_eq!(single(Layout::Fr, 'a'), KeyStroke::key(usage::Q));
        assert_eq!(single(Layout::Fr, 'q'), KeyStroke::key(usage::A));
        assert_eq!(single(Layout::Fr, 'z'), KeyStroke::key(usage::W));
        assert_eq!(single(Layout::Fr, 'w'), KeyStroke::key(usage::Z));
        assert_eq!(single(Layout::Fr, 'm'), KeyStroke::key(usage::SEMICOLON));
        assert_eq!(
            single(Layout::Fr, 'M'),
            KeyStroke::shifted(usage::SEMICOLON)
        );
        assert_eq!(single(Layout::Fr, 'b'), KeyStroke::key(usage::B));
        assert_eq!(single(Layout::Fr, '1'), KeyStroke::shifted(usage::KEY_1));
        assert_eq!(single(Layout::Fr, '0'), KeyStroke::shifted(usage::KEY_0));
    }

    #[test]
    fn fr_symbols() {
        assert_eq!(single(Layout::Fr, '@'), KeyStroke::alt_gr(usage::KEY_0));
        assert_eq!(single(Layout::Fr, '#'), KeyStroke::alt_gr(usage::KEY_3));
        assert_eq!(single(Layout::Fr, 'é'), KeyStroke::key(usage::KEY_2));
        assert_eq!(single(Layout::Fr, '.'), KeyStroke::shifted(usage::COMMA));
        assert_eq!(single(Layout::Fr, ','), KeyStroke::key(usage::M));
        assert_eq!(
            single(Layout::Fr, '<'),
            KeyStroke::key(usage::NON_US_BACKSLASH)
        );
        assert_eq!(single(Layout::Fr, '€'), KeyStroke::alt_gr(usage::E));
    }

    #[test]
    fn fr_dead_keys() {
        let seq = Layout::Fr.strokes('ê').unwrap();
        assert_eq!(
            seq.as_slice(),
            &[
                KeyStroke::key(usage::LEFT_BRACKET),
                KeyStroke::key(usage::E)
            ]
        );
        let seq = Layout::Fr.strokes('Â').unwrap();
        assert_eq!(
            seq.as_slice(),
            &[
                KeyStroke::key(usage::LEFT_BRACKET),
                KeyStroke::shifted(usage::Q)
            ]
        );
        let seq = Layout::Fr.strokes('~').unwrap();
        assert_eq!(
            seq.as_slice(),
            &[
                KeyStroke::alt_gr(usage::KEY_2),
                KeyStroke::key(usage::SPACE)
            ]
        );
    }

    #[test]
    fn layout_names() {
        assert_eq!(Layout::from_name("fr"), Some(Layout::Fr));
        assert_eq!(Layout::from_name("AZERTY"), Some(Layout::Fr));
        assert_eq!(Layout::from_name("us"), Some(Layout::Us));
        assert_eq!(Layout::from_name("de"), None);
    }
}
