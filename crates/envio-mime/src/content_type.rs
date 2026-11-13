//! The content type of a file to attach, guessed from its name.

/// The type for `file_name`'s extension (case-insensitive), or
/// `application/octet-stream` when it has none or is not in the table.
/// A name that is only a dot and an extension (`.pdf`) is a hidden file
/// with no extension, so it gets `application/octet-stream`.
pub fn content_type_for(file_name: &str) -> &'static str {
    let Some((stem, ext)) = file_name.rsplit_once('.') else {
        return OCTET;
    };
    if stem.is_empty() || stem.ends_with('/') {
        return OCTET;
    }
    let ext = ext.to_ascii_lowercase();
    TABLE
        .iter()
        .find(|(e, _)| *e == ext)
        .map_or(OCTET, |(_, t)| *t)
}

const OCTET: &str = "application/octet-stream";

const TABLE: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
    ("heic", "image/heic"),
    ("pdf", "application/pdf"),
    ("txt", "text/plain"),
    ("html", "text/html"),
    ("htm", "text/html"),
    ("ics", "text/calendar"),
    ("vcf", "text/vcard"),
    ("csv", "text/csv"),
    ("zip", "application/zip"),
    ("odt", "application/vnd.oasis.opendocument.text"),
    ("ods", "application/vnd.oasis.opendocument.spreadsheet"),
    ("odp", "application/vnd.oasis.opendocument.presentation"),
    (
        "docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    (
        "xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (
        "pptx",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    ("mp3", "audio/mpeg"),
    ("ogg", "audio/ogg"),
    ("m4a", "audio/mp4"),
    ("mp4", "video/mp4"),
    ("webm", "video/webm"),
];

#[cfg(test)]
mod tests {
    use super::content_type_for;

    #[test]
    fn the_extension_picks_the_type_whatever_its_case() {
        assert_eq!(content_type_for("x.PDF"), "application/pdf");
        assert_eq!(content_type_for("photo.JPeG"), "image/jpeg");
        assert_eq!(content_type_for("a.b.svg"), "image/svg+xml");
        assert_eq!(
            content_type_for("r.docx"),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
    }

    #[test]
    fn unknown_or_missing_extensions_are_octet_stream() {
        for name in ["noext", "a.unknownext", "trailing.", ""] {
            assert_eq!(content_type_for(name), "application/octet-stream", "{name}");
        }
    }

    #[test]
    fn a_dotfile_has_no_extension() {
        assert_eq!(content_type_for(".pdf"), "application/octet-stream");
    }

    #[test]
    fn every_type_passes_the_builders_validation() {
        for (_, t) in super::TABLE {
            let ok = t.split_once('/').is_some_and(|(k, s)| {
                [k, s].iter().all(|p| {
                    !p.is_empty()
                        && p.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "!#$&-^_.+".contains(c))
                })
            });
            assert!(ok, "{t}");
        }
    }
}
