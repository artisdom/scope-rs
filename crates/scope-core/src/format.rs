/// Render raw bytes as a mixed view:
/// - Printable ASCII (0x20..=0x7E) as characters
/// - Newline and carriage return as escapes ("\\n", "\\r")
/// - Everything else as hex escapes ("\\xHH")
///
/// This mirrors Scope's TUI behavior (useful when the serial stream is binary).
pub fn bytes_to_mixed_ascii(bytes: &[u8]) -> String {
    let mut out = String::new();

    for &b in bytes {
        match b {
            0x20..=0x7E => out.push(b as char),
            0x0a => out.push_str("\\n"),
            0x0d => out.push_str("\\r"),
            _ => {
                use core::fmt::Write;
                let _ = write!(&mut out, "\\x{b:02x}");
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::bytes_to_mixed_ascii;

    #[test]
    fn renders_printable() {
        assert_eq!(bytes_to_mixed_ascii(b"Hello"), "Hello");
    }

    #[test]
    fn renders_newlines() {
        assert_eq!(bytes_to_mixed_ascii(b"A\nB\r"), "A\\nB\\r");
    }

    #[test]
    fn renders_binary() {
        assert_eq!(bytes_to_mixed_ascii(&[0x00, 0x01, 0xff]), "\\x00\\x01\\xff");
    }
}
