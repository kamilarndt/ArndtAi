//! Schowek przez OSC 52 (runda 17).
//!
//! Sekwencja `ESC ] 52 ; c ; <base64> BEL` idzie prosto na stdout — działa
//! przez SSH i w tmuxie, bez `xclip` i bez zależności. Terminal nie odsyła
//! potwierdzenia, więc „potwierdzenie" to udany zapis sekwencji do prawdziwego
//! terminala; przy `TERM=dumb` (albo braku TTY) mówimy wprost, że schowek może
//! nie zadziałać, zamiast udawać sukces.

use std::io::{self, Write};

/// Co poszło nie tak przy kopiowaniu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardError {
    /// Terminal nie obsługuje OSC 52 (`TERM=dumb`, brak TTY).
    Unsupported,
    /// Zapis sekwencji do terminala się nie udał (zamknięty stdout).
    Io,
}

/// Sekwencja OSC 52 z tekstem zakodowanym base64 (schowek `c` — systemowy).
pub fn osc52_sequence(text: &str) -> String {
    format!("\u{1b}]52;c;{}\u{7}", base64(text.as_bytes()))
}

/// Zapis sekwencji do dowolnego strumienia — testy podają `Vec<u8>` jako
/// atrapę terminala, więc nie trzeba przechwytywać prawdziwego stdout.
pub fn write_osc52<W: Write>(out: &mut W, text: &str) -> io::Result<()> {
    out.write_all(osc52_sequence(text).as_bytes())?;
    out.flush()
}

/// Kopiowanie w TUI: przy `interactive` sekwencja idzie na stdout, w zrzutach
/// i testach tylko sprawdzamy, że terminal ją przyjął (bez wypisywania).
pub fn copy(text: &str, interactive: bool) -> Result<(), ClipboardError> {
    if !interactive {
        return Ok(());
    }
    let dumb = std::env::var("TERM")
        .map(|term| term.trim().is_empty() || term.trim() == "dumb")
        .unwrap_or(true);
    if dumb {
        return Err(ClipboardError::Unsupported);
    }
    write_osc52(&mut io::stdout(), text).map_err(|_| ClipboardError::Io)
}

/// Base64 (RFC 4648, z paddingiem) — 20 linii zamiast nowej zależności.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn osc52_sequence_has_the_expected_text_on_a_fake_terminal() {
        let mut terminal: Vec<u8> = Vec::new();
        write_osc52(&mut terminal, "raport gotowy").unwrap();
        let expected = format!(
            "\u{1b}]52;c;{}\u{7}",
            base64("raport gotowy".as_bytes())
        );
        assert_eq!(String::from_utf8(terminal).unwrap(), expected);
        // Polski tekst przechodzi przez UTF-8, nie przez lossy.
        let mut polish: Vec<u8> = Vec::new();
        write_osc52(&mut polish, "gotowe — 3 kroki").unwrap();
        let seq = String::from_utf8(polish).unwrap();
        assert!(seq.starts_with("\u{1b}]52;c;"));
        assert!(seq.ends_with('\u{7}'));
        assert!(seq.contains(&base64("gotowe — 3 kroki".as_bytes())));
    }

    #[test]
    fn non_interactive_copy_is_a_no_op_not_an_error() {
        // Zrzut i testy nie mogą nic wypisać na terminal — ale też nie mogą
        // zameldować porażki, bo nic nie próbowały.
        assert_eq!(copy("tekst", false), Ok(()));
    }
}
