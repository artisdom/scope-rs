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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    Reset,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    DarkGray,
    LightGreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub kind: SegmentKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSegment {
    pub text: String,
    pub kind: SegmentKind,
    pub color: AnsiColor,
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

pub fn bytes_to_ansi_segments(bytes: &[u8]) -> Vec<StyledSegment> {
    let patterns: [(&[u8], AnsiColor); 12] = [
        (b"\x1b[0m", AnsiColor::Reset),
        (b"\x1b[30m", AnsiColor::Black),
        (b"\x1b[31m", AnsiColor::Red),
        (b"\x1b[1;31m", AnsiColor::Red),
        (b"\x1b[32m", AnsiColor::Green),
        (b"\x1b[1;32m", AnsiColor::Green),
        (b"\x1b[33m", AnsiColor::Yellow),
        (b"\x1b[1;33m", AnsiColor::Yellow),
        (b"\x1b[34m", AnsiColor::Blue),
        (b"\x1b[35m", AnsiColor::Magenta),
        (b"\x1b[36m", AnsiColor::Cyan),
        (b"\x1b[37m", AnsiColor::White),
    ];

    let mut msg = bytes.to_vec();
    msg = replace_all(&msg, b"\x1b[m", b"");
    msg = replace_all(&msg, b"\x1b[8D", b"");
    msg = replace_all(&msg, b"\x1b[J", b"");

    let mut output: Vec<StyledSegment> = vec![];
    let mut buffer: Vec<u8> = vec![];
    let mut color = AnsiColor::Reset;

    for byte in msg {
        buffer.push(byte);

        if (byte as char) != 'm' {
            continue;
        }

        for (pattern, new_color) in patterns {
            if contains(&buffer, pattern) {
                let cleaned = replace_all(&buffer, pattern, b"");
                output.extend(bytes_to_string_segments(&cleaned, color));
                buffer.clear();
                color = new_color;
                break;
            }
        }
    }

    if !buffer.is_empty() {
        output.extend(bytes_to_string_segments(&buffer, color));
    }

    output
}

fn bytes_to_string_segments(msg: &[u8], color: AnsiColor) -> Vec<StyledSegment> {
    let mut output = vec![];
    let mut buffer = String::new();
    let mut in_plain_text = true;
    let accent_color = if color == AnsiColor::Yellow {
        AnsiColor::DarkGray
    } else {
        AnsiColor::Yellow
    };

    let flush = |out: &mut Vec<StyledSegment>, buf: &mut String, kind: SegmentKind, color| {
        if !buf.is_empty() {
            out.push(StyledSegment {
                text: core::mem::take(buf),
                kind,
                color,
            });
        }
    };

    for byte in msg {
        match *byte {
            x if (0x20..=0x7E).contains(&x) => {
                if !in_plain_text {
                    flush(&mut output, &mut buffer, SegmentKind::Escape, accent_color);
                    in_plain_text = true;
                }
                buffer.push(x as char);
            }
            x => {
                if in_plain_text {
                    flush(&mut output, &mut buffer, SegmentKind::Plain, color);
                    in_plain_text = false;
                }

                match x {
                    0x0a => buffer.push_str("\\n"),
                    0x0d => buffer.push_str("\\r"),
                    _ => {
                        use core::fmt::Write;
                        let _ = write!(&mut buffer, "\\x{byte:02x}");
                    }
                }
            }
        }
    }

    if !buffer.is_empty() {
        let final_color = if in_plain_text { color } else { accent_color };
        let final_kind = if in_plain_text {
            SegmentKind::Plain
        } else {
            SegmentKind::Escape
        };
        output.push(StyledSegment {
            text: buffer,
            kind: final_kind,
            color: final_color,
        });
    }

    output
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn replace_all(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;

    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            result.extend_from_slice(replacement);
            i += needle.len();
        } else {
            result.push(haystack[i]);
            i += 1;
        }
    }

    result.extend_from_slice(&haystack[i..]);
    result
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
