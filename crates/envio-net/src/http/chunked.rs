//! Chunked framing (RFC 9112 7.1) for token replies, read as the bytes
//! arrive: [`ResponseScan`] keeps its place between reads, so the work over a
//! whole reply is linear in its size, however the peer splits it.

use std::ops::Range;

use super::{Head, MAX_BODY, MAX_HEAD, head, length_complete};

/// The longest chunk-size line (size and extensions) read.
pub(super) const MAX_CHUNK_LINE: usize = 1024;

/// Where the body of a chunked reply is, or why there is none yet.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Chunked {
    /// The byte ranges of the bytes after the head that hold the body, in
    /// order, and their total length.
    Done(Vec<Range<usize>>, usize),
    /// A valid prefix: more bytes may complete it.
    Incomplete,
    /// Over `MAX_BODY` (a size declared, or the sum), seen before the data.
    TooBig,
    Malformed,
}

/// The line ending at the first CRLF from `at`: the index of that CRLF, or
/// `None` if the bytes so far hold none. A line over `limit` is `Err`. Only
/// `limit + 2` bytes are looked at.
fn line_end(rest: &[u8], at: usize, limit: usize) -> Result<Option<usize>, ()> {
    let window = &rest[at..rest.len().min(at + limit + 2)];
    match window.windows(2).position(|w| w == b"\r\n") {
        Some(i) => Ok(Some(at + i)),
        None if window.len() < limit + 2 => Ok(None),
        None => Err(()),
    }
}

/// Why a chunk-size line is not a size.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SizeError {
    Malformed,
    /// Hex digits that do not fit a `usize`.
    Overflow,
}

/// The size in a chunk-size line (hex digits, then optional `;extensions`).
pub(crate) fn chunk_size(line: &[u8]) -> Result<usize, SizeError> {
    let digits = line.split(|&b| b == b';').next().unwrap_or_default();
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_hexdigit) {
        return Err(SizeError::Malformed);
    }
    // All hex digits, so a failed parse is an overflow.
    std::str::from_utf8(digits)
        .ok()
        .and_then(|d| usize::from_str_radix(d, 16).ok())
        .ok_or(SizeError::Overflow)
}

/// The chunk reader's place: `at` is the start of the next chunk-size line
/// (or trailer line), which is only moved past a whole chunk.
#[derive(Default)]
pub(super) struct Dechunker {
    at: usize,
    total: usize,
    ranges: Vec<Range<usize>>,
    /// Where the trailers began, once the last chunk is read.
    trailers: Option<usize>,
    /// Bytes looked at so far, to show the work stays linear.
    pub(super) work: usize,
}

impl Dechunker {
    /// Reads on from where the last call stopped. `rest` is the bytes after
    /// the head, and only ever grows between calls. Hex sizes, extensions
    /// (ignored), data, and trailers (ignored, bounded by `MAX_HEAD`); every
    /// size is checked against `MAX_BODY` before its data is waited for. After
    /// any answer but `Incomplete`, the scan is over.
    pub(super) fn step(&mut self, rest: &[u8]) -> Chunked {
        loop {
            if let Some(start) = self.trailers {
                match line_end(rest, self.at, MAX_HEAD) {
                    Ok(Some(end)) if end == self.at => {
                        return Chunked::Done(std::mem::take(&mut self.ranges), self.total);
                    }
                    Ok(Some(end)) => {
                        self.work += end + 2 - self.at;
                        self.at = end + 2;
                    }
                    Ok(None) => return Chunked::Incomplete,
                    Err(()) => return Chunked::Malformed,
                }
                if self.at - start > MAX_HEAD {
                    return Chunked::Malformed;
                }
                continue;
            }
            let end = match line_end(rest, self.at, MAX_CHUNK_LINE) {
                Ok(Some(end)) => end,
                Ok(None) => return Chunked::Incomplete,
                Err(()) => return Chunked::Malformed,
            };
            self.work += end + 2 - self.at;
            let size = match chunk_size(&rest[self.at..end]) {
                Ok(size) => size,
                Err(SizeError::Malformed) => return Chunked::Malformed,
                Err(SizeError::Overflow) => return Chunked::TooBig,
            };
            let data = end + 2;
            if size == 0 {
                self.trailers = Some(data);
                self.at = data;
                continue;
            }
            if size > MAX_BODY || self.total + size > MAX_BODY {
                return Chunked::TooBig;
            }
            let data_end = data + size;
            if rest.len() < data_end + 2 {
                // The CRLF after the data is part of the chunk: check what of
                // it has arrived.
                let tail = &rest[data_end.min(rest.len())..];
                return if tail.iter().zip(b"\r\n").all(|(a, b)| a == b) {
                    Chunked::Incomplete
                } else {
                    Chunked::Malformed
                };
            }
            if &rest[data_end..data_end + 2] != b"\r\n" {
                return Chunked::Malformed;
            }
            self.ranges.push(data..data_end);
            self.total += size;
            self.at = data_end + 2;
        }
    }
}

/// All of `rest` at once.
pub(super) fn dechunk(rest: &[u8]) -> Chunked {
    Dechunker::default().step(rest)
}

/// Whether the bytes read so far hold all a response needs, for the reader
/// of `parse_response`: as `reply_complete`, but a chunked reply is complete
/// at its last chunk and trailers, or once its framing is known to be wrong
/// or over the cap (all refused at once). Each call looks only at what
/// arrived since the last.
#[derive(Default)]
pub(crate) struct ResponseScan {
    /// How far the head's end has been looked for.
    searched: usize,
    /// Bytes compared in that search, for the test of its linearity.
    head_work: usize,
    head: Option<Head>,
    chunks: Dechunker,
}

impl ResponseScan {
    pub(crate) fn complete(&mut self, raw: &[u8]) -> bool {
        if self.head.is_none() {
            let fresh = &raw[self.searched..];
            self.head_work += fresh.len();
            if !fresh.windows(4).any(|w| w == b"\r\n\r\n") {
                self.searched = raw.len().saturating_sub(3);
                return false;
            }
            self.head = head(raw);
        }
        let Some(head) = &self.head else {
            return false;
        };
        if head.bad_encoding {
            true
        } else if head.chunked {
            self.chunks.step(&raw[head.body_start..]) != Chunked::Incomplete
        } else {
            length_complete(head, raw.len())
        }
    }

    #[cfg(test)]
    pub(super) fn searched(&self) -> usize {
        self.searched
    }

    #[cfg(test)]
    pub(super) fn head_work(&self) -> usize {
        self.head_work
    }

    #[cfg(test)]
    pub(super) fn work(&self) -> usize {
        self.chunks.work
    }
}

/// [`ResponseScan`] over `raw` in one call.
#[cfg(test)]
pub(crate) fn response_complete(raw: &[u8]) -> bool {
    ResponseScan::default().complete(raw)
}
