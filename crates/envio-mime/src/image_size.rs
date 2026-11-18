//! The natural pixel size of an image, read from its header bytes. Pure:
//! no I/O, no decoding. `None` for an unknown format or a header that is
//! cut short or wrong.

/// Width and height in pixels of a PNG, GIF, JPEG or WebP, from the start of
/// the file. `None` when either is zero.
pub fn size(b: &[u8]) -> Option<(u32, u32)> {
    let s = if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(b)
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        gif(b)
    } else if b.starts_with(&[0xff, 0xd8]) {
        jpeg(b)
    } else if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        webp(b)
    } else {
        None
    };
    s.filter(|&(w, h)| w > 0 && h > 0)
}

fn le16(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 2)?;
    Some(u32::from(u16::from_le_bytes([s[0], s[1]])))
}

fn be16(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 2)?;
    Some(u32::from(u16::from_be_bytes([s[0], s[1]])))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 4)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

fn png(b: &[u8]) -> Option<(u32, u32)> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    be32(b, 16).zip(be32(b, 20))
}

/// The larger of the logical screen and the first frame, per axis: Qt grows
/// the canvas for a frame that does not fit the screen. The screen alone when
/// no frame header is in `b`.
fn gif(b: &[u8]) -> Option<(u32, u32)> {
    let screen = le16(b, 6).zip(le16(b, 8))?;
    let Some(&flags) = b.get(10) else {
        return Some(screen);
    };
    let mut at = 13;
    if flags & 0x80 != 0 {
        at += 3 << ((flags & 7) + 1);
    }
    // Skip extension blocks (0x21, label, sub-blocks ending in a 0 length).
    while b.get(at) == Some(&0x21) {
        at += 2;
        loop {
            let len = usize::from(*b.get(at)?);
            at += 1 + len;
            if len == 0 {
                break;
            }
        }
    }
    if b.get(at) != Some(&0x2c) {
        return Some(screen);
    }
    Some(match le16(b, at + 5).zip(le16(b, at + 7)) {
        Some((w, h)) => (screen.0.max(w), screen.1.max(h)),
        None => screen,
    })
}

/// Walks the segments to the first start-of-frame marker.
fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    loop {
        if *b.get(at)? != 0xff {
            return None;
        }
        let marker = *b.get(at + 1)?;
        match marker {
            0xff => at += 1,
            // Fill byte, TEM and RSTn have no length.
            0x00..=0x01 | 0xd0..=0xd7 => at += 2,
            // Start of image, end of image or start of scan, before any frame.
            0xd8..=0xda => return None,
            // SOF0..SOF15 but DHT, JPG and DAC.
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return be16(b, at + 7).zip(be16(b, at + 5));
            }
            _ => at += 2 + usize::try_from(be16(b, at + 2)?).ok()?,
        }
    }
}

fn webp(b: &[u8]) -> Option<(u32, u32)> {
    match b.get(12..16)? {
        b"VP8 " => {
            if b.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            let (w, h) = (le16(b, 26)?, le16(b, 28)?);
            Some((w & 0x3fff, h & 0x3fff))
        }
        b"VP8L" => {
            if *b.get(20)? != 0x2f {
                return None;
            }
            let s = b.get(21..25)?;
            let bits = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8X" => {
            let s = b.get(24..30)?;
            let le24 = |a: u8, b: u8, c: u8| u32::from(a) | u32::from(b) << 8 | u32::from(c) << 16;
            Some((le24(s[0], s[1], s[2]) + 1, le24(s[3], s[4], s[5]) + 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(b: &[u8]) -> Option<u32> {
        size(b).map(|s| s.0)
    }

    fn height(b: &[u8]) -> Option<u32> {
        size(b).map(|s| s.1)
    }

    fn png_of(w: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        v.extend(w.to_be_bytes());
        v.extend([0, 0, 0, 7, 8, 6, 0, 0, 0]);
        v
    }

    fn jpeg_of(w: u16) -> Vec<u8> {
        let mut v = vec![0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0];
        v.extend([0xff, 0xc4, 0, 2]);
        v.extend([0xff, 0xc2, 0, 11, 8, 0, 9]);
        v.extend(w.to_be_bytes());
        v.extend([1, 1, 0x11, 0]);
        v
    }

    #[test]
    fn png_width() {
        assert_eq!(width(&png_of(2000)), Some(2000));
        assert_eq!(height(&png_of(2000)), Some(7));
    }

    #[test]
    fn gif_width() {
        let mut v = b"GIF89a".to_vec();
        v.extend(700u16.to_le_bytes());
        v.extend(5u16.to_le_bytes());
        assert_eq!(size(&v), Some((700, 5)));
    }

    #[test]
    fn gif_frame_larger_than_the_screen_wins() {
        let mut v = b"GIF89a".to_vec();
        v.extend([10, 0, 10, 0, 0x80, 0, 0]);
        // A 2-entry global colour table, a comment extension, then a frame.
        v.extend([0; 6]);
        v.extend([0x21, 0xfe, 2, b'h', b'i', 0]);
        v.extend([0x2c, 0, 0, 0, 0]);
        v.extend(300u16.to_le_bytes());
        v.extend(4u16.to_le_bytes());
        v.push(0);
        assert_eq!(size(&v), Some((300, 10)));
    }

    #[test]
    fn jpeg_width_skips_other_segments_and_dht() {
        assert_eq!(size(&jpeg_of(1234)), Some((1234, 9)));
    }

    #[test]
    fn webp_lossy_lossless_and_extended() {
        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0".to_vec();
        lossy.extend([0x9d, 0x01, 0x2a]);
        lossy.extend(640u16.to_le_bytes());
        lossy.extend(480u16.to_le_bytes());
        assert_eq!(size(&lossy), Some((640, 480)));

        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        lossless.extend((799u32 | 99 << 14).to_le_bytes());
        assert_eq!(size(&lossless), Some((800, 100)));

        let mut ext = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        ext.extend([0xcf, 0x07, 0x00, 0x8f, 0x01, 0x00]);
        assert_eq!(size(&ext), Some((2000, 400)));
    }

    #[test]
    fn truncated_or_garbage_input_is_none() {
        assert_eq!(width(b""), None);
        assert_eq!(width(b"hello world, not an image"), None);
        let png = png_of(9);
        for n in 0..png.len().min(20) {
            assert_eq!(width(&png[..n]), None, "png {n}");
        }
        assert_eq!(width(&jpeg_of(9)[..14]), None);
        assert_eq!(width(&[0xff, 0xd8, 0xff]), None);
        assert_eq!(width(b"GIF89a\x01"), None);
        assert_eq!(width(b"RIFF\0\0\0\0WEBPVP8X\0\0"), None);
        let mut ext = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        ext.extend([1, 0, 0, 1, 0]);
        assert_eq!(size(&ext), None, "height cut short");
        assert_eq!(width(&png_of(0)), None);
    }
}
