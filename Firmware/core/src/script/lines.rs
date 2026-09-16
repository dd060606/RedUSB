//! Splits a byte stream into text lines without allocating.

/// Something that yields bytes sequentially, like a file reader.
pub trait ByteSource {
    type Error;

    /// Reads up to `buf.len()` bytes; 0 means end of stream.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error>;
}

impl ByteSource for &[u8] {
    type Error = core::convert::Infallible;

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let n = buf.len().min(self.len());
        buf[..n].copy_from_slice(&self[..n]);
        *self = &self[n..];
        Ok(n)
    }
}

/// Errors from [`LineReader::next_line`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineError<E> {
    Source(E),
    /// A line does not fit the reader's buffer.
    LineTooLong,
    InvalidUtf8,
}

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Buffered line splitter. `N` bounds the length of one line.
pub struct LineReader<const N: usize> {
    buf: [u8; N],
    len: usize,
    pos: usize,
    eof: bool,
    at_start: bool,
}

impl<const N: usize> Default for LineReader<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> LineReader<N> {
    pub fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
            pos: 0,
            eof: false,
            at_start: true,
        }
    }

    /// Returns the next line without its terminator, or `None` at end of stream.
    /// A UTF-8 byte order mark at the very start is skipped.
    pub fn next_line<S: ByteSource>(
        &mut self,
        src: &mut S,
    ) -> Result<Option<&str>, LineError<S::Error>> {
        loop {
            if self.at_start && self.len - self.pos >= UTF8_BOM.len() {
                if self.buf[self.pos..self.pos + 3] == UTF8_BOM {
                    self.pos += 3;
                }
                self.at_start = false;
            }
            let pending = &self.buf[self.pos..self.len];
            if let Some(i) = pending.iter().position(|&b| b == b'\n') {
                let start = self.pos;
                self.pos += i + 1;
                return Self::decode(&self.buf[start..start + i]).map(Some);
            }
            if self.eof {
                if self.pos == self.len {
                    return Ok(None);
                }
                let start = self.pos;
                self.pos = self.len;
                return Self::decode(&self.buf[start..self.len]).map(Some);
            }
            // Need more bytes: compact then refill.
            if self.pos > 0 {
                self.buf.copy_within(self.pos..self.len, 0);
                self.len -= self.pos;
                self.pos = 0;
            }
            if self.len == N {
                return Err(LineError::LineTooLong);
            }
            let n = src
                .read(&mut self.buf[self.len..])
                .map_err(LineError::Source)?;
            if n == 0 {
                self.eof = true;
                // A tiny file may be shorter than the BOM check threshold.
                self.at_start = false;
            } else {
                self.len += n;
            }
        }
    }

    fn decode<E>(bytes: &[u8]) -> Result<&str, LineError<E>> {
        let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
        core::str::from_utf8(bytes).map_err(|_| LineError::InvalidUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::string::String;
    use std::vec::Vec;

    fn collect<const N: usize>(
        input: &[u8],
    ) -> Result<Vec<String>, LineError<core::convert::Infallible>> {
        let mut src = input;
        let mut reader = LineReader::<N>::new();
        let mut out = Vec::new();
        while let Some(line) = reader.next_line(&mut src)? {
            out.push(String::from(line));
        }
        Ok(out)
    }

    #[test]
    fn splits_lf_and_crlf() {
        let lines = collect::<64>(b"one\r\ntwo\nthree").unwrap();
        assert_eq!(lines, ["one", "two", "three"]);
    }

    #[test]
    fn handles_empty_lines_and_trailing_newline() {
        let lines = collect::<64>(b"a\n\nb\n").unwrap();
        assert_eq!(lines, ["a", "", "b"]);
        assert!(collect::<64>(b"").unwrap().is_empty());
    }

    #[test]
    fn skips_bom() {
        let lines = collect::<64>(b"\xEF\xBB\xBFREM x\nSTRING y").unwrap();
        assert_eq!(lines, ["REM x", "STRING y"]);
        // A file shorter than a BOM must still work.
        assert_eq!(collect::<64>(b"ab").unwrap(), ["ab"]);
    }

    #[test]
    fn lines_crossing_refills() {
        // Buffer of 8 bytes, lines of varying length forcing compaction.
        let lines = collect::<8>(b"abc\ndefgh\ni\njk").unwrap();
        assert_eq!(lines, ["abc", "defgh", "i", "jk"]);
    }

    #[test]
    fn too_long_line_is_reported() {
        assert_eq!(
            collect::<8>(b"123456789\n").unwrap_err(),
            LineError::LineTooLong
        );
    }

    #[test]
    fn invalid_utf8_is_reported() {
        assert_eq!(
            collect::<64>(b"ok\n\xFF\xFE\n").unwrap_err(),
            LineError::InvalidUtf8
        );
    }
}
