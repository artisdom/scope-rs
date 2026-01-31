/// Render raw bytes as a mixed view:
/// - Printable ASCII (0x20..=0x7E) as characters
/// - Newline and carriage return as escapes ("\\n", "\\r")
/// - Everything else as hex escapes ("\\xHH")
///
/// This mirrors Scope's TUI behavior (useful when the serial stream is binary).
pub fn bytes_to_mixed_ascii(bytes: &[u8]) -> String {
    bytes_to_mixed_segments(bytes)
        .into_iter()
        .map(|s| s.text)
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Plain,
    Escape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub kind: SegmentKind,
}

/// Split bytes into segments so UIs can apply different styling.
///
/// - `Plain` contains consecutive printable ASCII.
/// - `Escape` contains consecutive escaped sequences (\n, \r, \xHH).
pub fn bytes_to_mixed_segments(bytes: &[u8]) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();

    let mut buf = String::new();
    let mut kind = SegmentKind::Plain;

    let flush = |out: &mut Vec<Segment>, buf: &mut String, kind: SegmentKind| {
        if !buf.is_empty() {
            out.push(Segment {
                text: core::mem::take(buf),
                kind,
            });
        }
    };

    for &b in bytes {
        match b {
            0x20..=0x7E => {
                if kind != SegmentKind::Plain {
                    flush(&mut out, &mut buf, kind);
                    kind = SegmentKind::Plain;
                }
                buf.push(b as char);
            }
            0x0a => {
                if kind != SegmentKind::Escape {
                    flush(&mut out, &mut buf, kind);
                    kind = SegmentKind::Escape;
                }
                buf.push_str("\\n");
            }
            0x0d => {
                if kind != SegmentKind::Escape {
                    flush(&mut out, &mut buf, kind);
                    kind = SegmentKind::Escape;
                }
                buf.push_str("\\r");
            }
            _ => {
                if kind != SegmentKind::Escape {
                    flush(&mut out, &mut buf, kind);
                    kind = SegmentKind::Escape;
                }
                use core::fmt::Write;
                let _ = write!(&mut buf, "\\x{b:02x}");
            }
        }
    }

    flush(&mut out, &mut buf, kind);
    out
}

#[cfg(test)]
mod tests {
    use super::{bytes_to_mixed_ascii, bytes_to_mixed_segments, SegmentKind};

    #[test]
    fn renders_printable() {
        assert_eq!(bytes_to_mixed_ascii(b"Hello"), "Hello");
        let segs = bytes_to_mixed_segments(b"Hello");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].kind, SegmentKind::Plain);
    }

    #[test]
    fn renders_newlines() {
        assert_eq!(bytes_to_mixed_ascii(b"A\nB\r"), "A\\nB\\r");
    }

    #[test]
    fn renders_binary() {
        assert_eq!(bytes_to_mixed_ascii(&[0x00, 0x01, 0xff]), "\\x00\\x01\\xff");
    }

    #[test]
    fn segments_split_plain_and_escape() {
        let segs = bytes_to_mixed_segments(b"A\nB\x00C");
        assert!(segs.iter().any(|s| s.kind == SegmentKind::Plain));
        assert!(segs.iter().any(|s| s.kind == SegmentKind::Escape));
    }
}
