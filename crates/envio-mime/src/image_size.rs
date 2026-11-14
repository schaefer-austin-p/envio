//! The natural pixel width of an image, read from its header bytes. Pure:
//! no I/O, no decoding. `None` for an unknown format or a header that is
//! cut short or wrong.

/// Width in pixels of a PNG, GIF, JPEG or WebP, from the start of the file.
pub fn width(b: &[u8]) -> Option<u32> {
    let w = if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(b)
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        le16(b, 6)
    } else if b.starts_with(&[0xff, 0xd8]) {
        jpeg(b)
    } else if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
        webp(b)
    } else {
        None
    };
    w.filter(|&w| w > 0)
}

fn le16(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 2)?;
    Some(u32::from(u16::from_le_bytes([s[0], s[1]])))
}

fn be16(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at + 2)?;
    Some(u32::from(u16::from_be_bytes([s[0], s[1]])))
}

fn png(b: &[u8]) -> Option<u32> {
    if b.get(12..16)? != b"IHDR" {
        return None;
    }
    let s = b.get(16..20)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// Walks the segments to the first start-of-frame marker.
fn jpeg(b: &[u8]) -> Option<u32> {
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
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => return be16(b, at + 7),
            _ => at += 2 + usize::try_from(be16(b, at + 2)?).ok()?,
        }
    }
}

fn webp(b: &[u8]) -> Option<u32> {
    match b.get(12..16)? {
        b"VP8 " => {
            if b.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            le16(b, 26).map(|w| w & 0x3fff)
        }
        b"VP8L" => {
            if *b.get(20)? != 0x2f {
                return None;
            }
            le16(b, 21).map(|w| (w & 0x3fff) + 1)
        }
        b"VP8X" => {
            let s = b.get(24..27)?;
            Some((u32::from(s[0]) | u32::from(s[1]) << 8 | u32::from(s[2]) << 16) + 1)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn gif_width() {
        let mut v = b"GIF89a".to_vec();
        v.extend(700u16.to_le_bytes());
        v.extend(5u16.to_le_bytes());
        assert_eq!(width(&v), Some(700));
    }

    #[test]
    fn jpeg_width_skips_other_segments_and_dht() {
        assert_eq!(width(&jpeg_of(1234)), Some(1234));
    }

    #[test]
    fn webp_lossy_lossless_and_extended() {
        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0".to_vec();
        lossy.extend([0x9d, 0x01, 0x2a]);
        lossy.extend(640u16.to_le_bytes());
        lossy.extend(480u16.to_le_bytes());
        assert_eq!(width(&lossy), Some(640));

        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        lossless.extend((799u16 | 99 << 14).to_le_bytes());
        assert_eq!(width(&lossless), Some(800));

        let mut ext = b"RIFF\0\0\0\0WEBPVP8X\0\0\0\0\0\0\0\0".to_vec();
        ext.extend([0xcf, 0x07, 0x00]);
        assert_eq!(width(&ext), Some(2000));
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
        assert_eq!(width(&png_of(0)), None);
    }
}
