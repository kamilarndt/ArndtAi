//! Markdown w odpowiedziach asystenta (runda 18).
//!
//! Wejście: surowy tekst modelu. Wyjście: gotowe wiersze (spany + tło), które
//! `tui.rs` wstawia w scrollback bez własnej interpretacji treści.
//!
//! Zasady z `docs/re/tui-parity.md` i zlecenia rundy 18:
//!
//! - nagłówki `#`–`###` → pogrubiony tekst bez hashy,
//! - `**x**` → bold, `*x*` / `_x_` → kursywa, `` `x` `` → inne tło (ta sama paleta),
//! - listy `-` / `1.` → wcięcie + punktor `•` (numeracja w listach numerowanych),
//! - blok ``` z językiem → ramka z etykietą w prawym górnym narożniku
//!   i podświetlaniem dla `rust`, `json`, `bash`, `toml`,
//! - cytat `>` → pionowa kreska z lewej, przygaszony tekst,
//! - tabela `| a | b |` → prosta siatka `│`, bez zawijania kolumn.
//!
//! Świadomie pominięte (zostają dosłownym tekstem): obrazy, klikalne linki,
//! HTML, LaTeX, mermaid, emoji zamieniane na ikony. Link zostaje tekstem
//! podkreślonym plus adres w nawiasie, obraz — sam podpis plus adres.
//!
//! Renderer nie panikuje na niepełnym markdownie: brak zamknięcia `**`, `*`,
//! `` ` `` albo ``` zostawia znaki tak, jak przyszły.

use crate::tui::{BG_LIGHT, FG, FG_DARK, GRAY, GRAY_DIM, GREEN, MAGENTA, ORANGE};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Od tej szerokości ramki kodu tracą prawą krawędź (zlecenie: „< 40 kolumn”).
pub const NARROW_WIDTH: usize = 40;
/// Powyżej tylu linii blok kodu jest obcinany z `… (N wiecej)`.
pub const CODE_MAX_LINES: usize = 200;
/// Reguła `---` — ten sam znak co w `tui.rs` (`RULE`).
pub const RULE_TEXT: &str = "\u{2014}\u{2014}";
/// Punktor listy nienumerowanej.
pub const BULLET: char = '\u{2022}'; // •

const KEYWORD: Color = MAGENTA;
const STRING: Color = GREEN;
const NUMBER: Color = ORANGE;
const COMMENT: Color = GRAY_DIM;

/// Jeden gotowy wiersz: tło całego wiersza (dziś zawsze `None`) i spany.
#[derive(Clone, Debug)]
pub struct Line {
    pub bg: Option<Color>,
    pub spans: Vec<Span<'static>>,
}

impl Line {
    fn blank() -> Line {
        Line {
            bg: None,
            spans: Vec::new(),
        }
    }

    fn plain(spans: Vec<Span<'static>>) -> Line {
        Line { bg: None, spans }
    }

    /// Cały tekst wiersza — testy i szukanie liczą po nim, nie po stylach.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.content.as_ref()).collect()
    }
}

/// Podstawowe style treści — kolory wyłącznie z palety `groknight`, żeby
/// motyw `jasny` (`theme::recolor`) miał dla nich odpowiednik.
fn body() -> Style {
    Style::default().fg(FG_DARK)
}

fn bold() -> Style {
    Style::default().fg(FG).add_modifier(Modifier::BOLD)
}

fn italic() -> Style {
    Style::default().fg(FG_DARK).add_modifier(Modifier::ITALIC)
}

fn code_style() -> Style {
    Style::default().fg(FG).bg(BG_LIGHT)
}

fn dim() -> Style {
    Style::default().fg(GRAY)
}

fn frame_style() -> Style {
    Style::default().fg(GRAY_DIM)
}

/// Szerokość w kolumnach. Liczymy znaki, nie bajty i nie szerokość CJK —
/// tak samo robi reszta TUI (`wrap`, `truncate` w `tui.rs`).
fn width_of(text: &str) -> usize {
    text.chars().count()
}

fn spaces(n: usize) -> String {
    " ".repeat(n)
}

// ── Wejście ─────────────────────────────────────────────────────────────────

/// Renderuje tekst asystenta do wierszy o zadanej szerokości.
pub fn render(text: &str, width: usize) -> Vec<Line> {
    let width = width.max(8);
    let mut out: Vec<Line> = Vec::new();
    let mut pending_blank = false;
    let source: Vec<&str> = text
        .split('\n')
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let mut i = 0;
    while i < source.len() {
        let raw = source[i];
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            // Pusty wiersz tylko między treścią — na końcu i na początku nie.
            pending_blank = !out.is_empty();
            i += 1;
            continue;
        }
        if pending_blank {
            out.push(Line::blank());
            pending_blank = false;
        }
        if let Some((fence, info)) = fence_start(raw) {
            i += 1;
            let mut body_lines: Vec<String> = Vec::new();
            while i < source.len() {
                let line = source[i];
                if is_fence_end(line, fence) {
                    i += 1;
                    break;
                }
                body_lines.push(line.to_string());
                i += 1;
            }
            out.extend(code_rows(&body_lines, &info, width));
            continue;
        }
        if is_table_row(raw) && i + 1 < source.len() && is_table_rule(source[i + 1]) {
            let mut rows: Vec<String> = Vec::new();
            while i < source.len() && is_table_row(source[i]) {
                rows.push(source[i].to_string());
                i += 1;
            }
            out.extend(table_rows(&rows, width));
            continue;
        }
        if let Some((level, rest)) = heading(trimmed) {
            let style = match level {
                1 | 2 => bold(),
                3 => Style::default().fg(FG).add_modifier(Modifier::BOLD),
                _ => Style::default().fg(GRAY).add_modifier(Modifier::BOLD),
            };
            out.extend(wrap_segments(
                inline_segments(rest, style, 0),
                width,
                Vec::new(),
                Vec::new(),
            ));
            i += 1;
            continue;
        }
        if is_rule(trimmed) {
            out.push(Line::plain(vec![Span::styled(RULE_TEXT, frame_style())]));
            i += 1;
            continue;
        }
        if let Some(item) = list_item(raw) {
            out.extend(list_rows(&item, width));
            i += 1;
            continue;
        }
        if let Some((depth, rest)) = quote_text(raw) {
            out.extend(quote_rows(depth, rest, width));
            i += 1;
            continue;
        }
        // Zwykły akapit: każda linia źródła to osobny wiersz (twarde złamanie),
        // zawijana na szerokości terminala.
        out.extend(wrap_segments(
            inline_segments(trimmed, body(), 0),
            width,
            Vec::new(),
            Vec::new(),
        ));
        i += 1;
    }
    while out.last().map(|line| line.spans.is_empty()).unwrap_or(false) {
        out.pop();
    }
    out
}

// ── Bloki ───────────────────────────────────────────────────────────────────

fn fence_start(line: &str) -> Option<(&'static str, String)> {
    let trimmed = line.trim_start();
    for fence in ["```", "~~~"] {
        if let Some(info) = trimmed.strip_prefix(fence) {
            // Info string: pierwsze słowo, np. ```rust
            let lang = info.split_whitespace().next().unwrap_or("").to_string();
            return Some((fence, lang));
        }
    }
    None
}

fn is_fence_end(line: &str, fence: &str) -> bool {
    line.trim_start().starts_with(fence)
}

/// `# Nagłówek` … `###### Nagłówek` → (poziom, tekst bez hashy).
fn heading(line: &str) -> Option<(usize, &str)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = line.get(hashes..)?;
    rest.strip_prefix(' ').map(|text| (hashes, text.trim_end()))
}

fn is_rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3
        && (compact.chars().all(|c| c == '-')
            || compact.chars().all(|c| c == '*')
            || compact.chars().all(|c| c == '_'))
}

struct ListItem {
    depth: usize,
    marker: String,
    text: String,
}

/// `- punkt`, `* punkt`, `+ punkt`, `1. punkt`, `1) punkt` z wcięciem.
fn list_item(line: &str) -> Option<ListItem> {
    let indent = line.chars().take_while(|c| *c == ' ').count();
    let trimmed = line.trim_start();
    let depth = (indent / 2).min(4);
    for prefix in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some(ListItem {
                depth,
                marker: format!("{} ", BULLET),
                text: rest.trim_end().to_string(),
            });
        }
    }
    // `12. tekst` — cyfry, kropka albo nawias, spacja.
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && digits <= 3 {
        let rest = &trimmed[digits..];
        for sep in [". ", ") "] {
            if let Some(text) = rest.strip_prefix(sep) {
                return Some(ListItem {
                    depth,
                    marker: format!("{}{} ", &trimmed[..digits], sep.trim_end()),
                    text: text.trim_end().to_string(),
                });
            }
        }
    }
    None
}

/// `> cytat`, `>> głębiej` → (głębokość, treść).
fn quote_text(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('>') {
        return None;
    }
    let mut rest = trimmed;
    let mut depth = 0;
    while let Some(after) = rest.strip_prefix('>') {
        depth += 1;
        rest = after.strip_prefix(' ').unwrap_or(after);
    }
    Some((depth.min(3), rest.trim_end()))
}

fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.chars().filter(|c| *c == '|').count() >= 2
}

/// `|---|:--:|` — wiersz rozdzielający nagłówek od treści.
fn is_table_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if !is_table_row(trimmed) {
        return false;
    }
    let inner = trimmed.trim_matches('|');
    !inner.trim().is_empty()
        && inner.split('|').all(|cell| {
            let cell = cell.trim();
            !cell.is_empty() && cell.chars().all(|c| c == '-' || c == ':')
        })
}

fn table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    inner.split('|').map(|cell| cell.trim().to_string()).collect()
}

// ── Bloki: lista, cytat ─────────────────────────────────────────────────────

fn list_rows(item: &ListItem, width: usize) -> Vec<Line> {
    let indent = spaces(item.depth * 2);
    let marker = &item.marker;
    let prefix = vec![
        Span::styled(indent.clone(), dim()),
        Span::styled(marker.clone(), dim()),
    ];
    let rest = vec![Span::styled(spaces(indent.len() + marker.len()), dim())];
    let mut lines = wrap_segments(
        inline_segments(&item.text, body(), 0),
        width,
        prefix,
        rest,
    );
    if lines.is_empty() {
        lines.push(Line::plain(vec![Span::styled(
            format!("{}{}", indent, marker),
            dim(),
        )]));
    }
    lines
}

fn quote_rows(depth: usize, text: &str, width: usize) -> Vec<Line> {
    let mut bar = String::new();
    for _ in 0..depth.max(1) {
        bar.push('\u{2502}'); // │
        bar.push(' ');
    }
    let style = Style::default().fg(GRAY);
    let prefix = vec![Span::styled(bar.clone(), frame_style())];
    let cont = vec![Span::styled(bar.clone(), frame_style())];
    let segments = inline_segments(text, style, 0)
        .into_iter()
        .map(|(text, style)| (text, style))
        .collect();
    let mut lines = wrap_segments(segments, width, prefix, cont);
    if lines.is_empty() {
        lines.push(Line::plain(vec![Span::styled(bar, frame_style())]));
    }
    lines
}

// ── Blok kodu ───────────────────────────────────────────────────────────────

fn code_rows(lines: &[String], lang: &str, width: usize) -> Vec<Line> {
    let narrow = width < NARROW_WIDTH;
    let mut out: Vec<Line> = Vec::new();
    let label = if lang.trim().is_empty() {
        String::new()
    } else {
        format!(" {} ", lang.trim())
    };
    let label_w = width_of(&label);
    let total = lines.len();
    let shown = total.min(CODE_MAX_LINES);
    let inner = if narrow {
        width.saturating_sub(2).max(1)
    } else {
        width.saturating_sub(4).max(1)
    };

    // Górna krawędź: `┌──────── rust ┐` (etykieta przy prawym narożniku).
    if narrow {
        // Bez prawej krawędzi: same myślniki z etykietą przy końcu wiersza.
        let pad = width.saturating_sub(label_w);
        let mut top = "\u{2500}".repeat(pad);
        top.push_str(&label);
        out.push(Line::plain(vec![Span::styled(top, frame_style())]));
    } else {
        let room = width.saturating_sub(2);
        let label = if label_w + 1 <= room {
            label.clone()
        } else {
            String::new()
        };
        let mut top = String::from("\u{250C}");
        top.push_str(&"\u{2500}".repeat(room.saturating_sub(width_of(&label))));
        top.push_str(&label);
        top.push('\u{2510}');
        out.push(Line::plain(vec![Span::styled(top, frame_style())]));
    }

    for raw in lines.iter().take(shown) {
        let (visible, hidden) = clip_code_line(raw, inner);
        let mut spans: Vec<Span<'static>> = vec![Span::styled("\u{2502} ", frame_style())];
        let mut highlighted = highlight(lang_of(lang), &visible);
        if let Some(suffix) = hidden {
            highlighted.push(Span::styled(suffix, frame_style()));
        }
        let used: usize = highlighted.iter().map(|s| width_of(&s.content)).sum();
        spans.extend(highlighted);
        if !narrow {
            if used < inner {
                spans.push(Span::styled(spaces(inner - used), body()));
            }
            // Prawa krawędź zawsze — także gdy treść wypełnia ramkę co do znaku.
            spans.push(Span::styled(" \u{2502}", frame_style()));
        }
        out.push(Line::plain(spans));
    }

    if total > shown {
        let more = format!(" \u{2026} ({} wiecej)", total - shown);
        if narrow {
            out.push(Line::plain(vec![
                Span::styled("\u{2502} ", frame_style()),
                Span::styled(more.trim_start().to_string(), dim()),
            ]));
        } else {
            let used = width_of(&more);
            out.push(Line::plain(vec![
                Span::styled("\u{2502} ", frame_style()),
                Span::styled(more.clone(), dim()),
                Span::styled(spaces(inner.saturating_sub(used)), body()),
                Span::styled(" \u{2502}", frame_style()),
            ]));
        }
    }

    // Dolna krawędź: pełna szerokość w trybie szerokim, same myślniki w wąskim.
    if narrow {
        out.push(Line::plain(vec![Span::styled(
            "\u{2500}".repeat(width),
            frame_style(),
        )]));
    } else {
        let mut bottom = String::from("\u{2514}");
        bottom.push_str(&"\u{2500}".repeat(width.saturating_sub(2)));
        bottom.push('\u{2518}');
        out.push(Line::plain(vec![Span::styled(bottom, frame_style())]));
    }
    out
}

/// Linia kodu szersza niż `inner` jest ucinana z `…` i liczbą ukrytych znaków.
fn clip_code_line(text: &str, inner: usize) -> (String, Option<String>) {
    let count = width_of(text);
    if count <= inner {
        return (text.to_string(), None);
    }
    for keep in (0..=inner).rev() {
        let hidden = count - keep;
        let suffix = format!(" \u{2026} +{} znakow", hidden);
        if keep + width_of(&suffix) <= inner {
            let head: String = text.chars().take(keep).collect();
            return (head, Some(suffix));
        }
    }
    // Wąska ramka: krótsza forma, a gdy i ta nie wchodzi — sam wielokropek.
    for keep in (0..=inner).rev() {
        let hidden = count - keep;
        let suffix = format!(" \u{2026} +{}", hidden);
        if keep + width_of(&suffix) <= inner {
            let head: String = text.chars().take(keep).collect();
            return (head, Some(suffix));
        }
    }
    let head: String = text.chars().take(inner.saturating_sub(1)).collect();
    (format!("{}\u{2026}", head), None)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    Json,
    Bash,
    Toml,
    Plain,
}

fn lang_of(info: &str) -> Lang {
    match info.trim().to_ascii_lowercase().as_str() {
        "rust" | "rs" => Lang::Rust,
        "json" | "jsonc" => Lang::Json,
        "bash" | "sh" | "shell" | "zsh" | "console" => Lang::Bash,
        "toml" => Lang::Toml,
        _ => Lang::Plain,
    }
}

fn keywords(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Rust => &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
            "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
            "trait", "true", "type", "unsafe", "use", "where", "while", "union", "yield",
        ],
        Lang::Json => &["true", "false", "null"],
        Lang::Bash => &[
            "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case",
            "esac", "in", "function", "local", "export", "return", "exit", "set", "unset",
            "readonly", "declare", "shift", "trap", "source", "alias", "echo", "cd", "printf",
            "true", "false",
        ],
        Lang::Toml => &["true", "false"],
        Lang::Plain => &[],
    }
}

/// Podświetlanie jednej linii: komentarze, stringi, liczby, słowa kluczowe.
/// Inne języki (`Lang::Plain`) idą jednym kolorem — bez zgadywania.
fn highlight(lang: Lang, line: &str) -> Vec<Span<'static>> {
    if lang == Lang::Plain || line.is_empty() {
        return vec![Span::styled(line.to_string(), Style::default().fg(FG))];
    }
    let bytes = line.as_bytes();
    let mut marks: Vec<(usize, usize, Color)> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match lang {
            Lang::Json => {
                if c == b'"' {
                    let end = scan_string(bytes, i);
                    marks.push((i, end, STRING));
                    i = end;
                    continue;
                }
                if c.is_ascii_digit() || (c == b'-' && bytes.get(i + 1).map_or(false, |d| d.is_ascii_digit())) {
                    let end = scan_number(bytes, i);
                    marks.push((i, end, NUMBER));
                    i = end;
                    continue;
                }
                if is_ident_start(c) {
                    let end = scan_ident(bytes, i);
                    if keywords(lang).contains(&&line[i..end]) {
                        marks.push((i, end, KEYWORD));
                    }
                    i = end;
                    continue;
                }
            }
            Lang::Toml => {
                if c == b'#' {
                    marks.push((i, bytes.len(), COMMENT));
                    break;
                }
                if c == b'"' || c == b'\'' {
                    let end = scan_quoted(bytes, i, c);
                    marks.push((i, end, STRING));
                    i = end;
                    continue;
                }
                if c == b'[' {
                    if let Some(close) = line[i..].find(']') {
                        marks.push((i, i + close + 1, KEYWORD));
                        i += close + 1;
                        continue;
                    }
                }
                if c.is_ascii_digit() {
                    let end = scan_number(bytes, i);
                    marks.push((i, end, NUMBER));
                    i = end;
                    continue;
                }
                if is_ident_start(c) {
                    let end = scan_ident(bytes, i);
                    if keywords(lang).contains(&&line[i..end]) {
                        marks.push((i, end, KEYWORD));
                    }
                    i = end;
                    continue;
                }
            }
            Lang::Bash => {
                if c == b'#' && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') {
                    marks.push((i, bytes.len(), COMMENT));
                    break;
                }
                if c == b'"' || c == b'\'' {
                    let end = scan_quoted(bytes, i, c);
                    marks.push((i, end, STRING));
                    i = end;
                    continue;
                }
                if c.is_ascii_digit() {
                    let end = scan_number(bytes, i);
                    marks.push((i, end, NUMBER));
                    i = end;
                    continue;
                }
                if is_ident_start(c) {
                    let end = scan_ident(bytes, i);
                    if keywords(lang).contains(&&line[i..end]) {
                        marks.push((i, end, KEYWORD));
                    }
                    i = end;
                    continue;
                }
            }
            Lang::Rust => {
                if c == b'/' && bytes.get(i + 1) == Some(&b'/') {
                    marks.push((i, bytes.len(), COMMENT));
                    break;
                }
                if c == b'"' {
                    let end = scan_string(bytes, i);
                    marks.push((i, end, STRING));
                    i = end;
                    continue;
                }
                if c == b'\'' {
                    if let Some(end) = scan_char(bytes, i) {
                        marks.push((i, end, STRING));
                        i = end;
                        continue;
                    }
                    i += 1;
                    continue;
                }
                if c.is_ascii_digit() {
                    let end = scan_number(bytes, i);
                    marks.push((i, end, NUMBER));
                    i = end;
                    continue;
                }
                if is_ident_start(c) {
                    let end = scan_ident(bytes, i);
                    if keywords(lang).contains(&&line[i..end]) {
                        marks.push((i, end, KEYWORD));
                    }
                    i = end;
                    continue;
                }
            }
            Lang::Plain => {}
        }
        i += 1;
    }
    spans_from(line, &marks)
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn scan_ident(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && is_ident_char(bytes[i]) {
        i += 1;
    }
    i.max(start + 1).min(bytes.len())
}

/// `"…"` z obsługą `\"`; brak zamknięcia → do końca linii.
fn scan_string(bytes: &[u8], start: usize) -> usize {
    scan_quoted(bytes, start, b'"')
}

fn scan_quoted(bytes: &[u8], start: usize, quote: u8) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        let c = bytes[i];
        if quote == b'"' && c == b'\\' {
            i += 2;
            continue;
        }
        if c == quote {
            return i + 1;
        }
        i += 1;
    }
    bytes.len()
}

/// `'x'`, `'\n'` — tylko literał znaku; `'a` (lifetime) zostaje bez koloru.
fn scan_char(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start + 1) == Some(&b'\\') {
        let mut i = start + 3;
        while i < bytes.len() && i <= start + 5 {
            if bytes[i] == b'\'' {
                return Some(i + 1);
            }
            i += 1;
        }
        return None;
    }
    if bytes.get(start + 2) == Some(&b'\'') && bytes.get(start + 1) != Some(&b'\'') {
        return Some(start + 3);
    }
    None
}

fn scan_number(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    if bytes[i] == b'-' {
        i += 1;
    }
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'.' {
            i += 1;
        } else {
            break;
        }
    }
    i.max(start + 1).min(bytes.len())
}

fn spans_from(line: &str, marks: &[(usize, usize, Color)]) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut at = 0usize;
    for (start, end, color) in marks {
        let (start, end) = (*start, (*end).min(line.len()));
        if start >= end || start < at {
            continue;
        }
        if start > at {
            spans.push(Span::styled(
                line[at..start].to_string(),
                Style::default().fg(FG),
            ));
        }
        spans.push(Span::styled(
            line[start..end].to_string(),
            Style::default().fg(*color),
        ));
        at = end;
    }
    if at < line.len() {
        spans.push(Span::styled(line[at..].to_string(), Style::default().fg(FG)));
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), Style::default().fg(FG)));
    }
    spans
}

// ── Tabela ──────────────────────────────────────────────────────────────────

/// Prosta siatka `│`, kolumny bez zawijania: szerokość kolumny to najszersza
/// komórka (przycięta, gdy cała tabela nie mieści się w terminalu).
fn table_rows(lines: &[String], width: usize) -> Vec<Line> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (index, raw) in lines.iter().enumerate() {
        if index == 1 && is_table_rule(raw) {
            continue;
        }
        rows.push(table_cells(raw));
    }
    if rows.is_empty() {
        return Vec::new();
    }
    let cols = rows.iter().map(|row| row.len()).max().unwrap_or(0).max(1);
    for row in rows.iter_mut() {
        while row.len() < cols {
            row.push(String::new());
        }
    }
    let mut widths: Vec<usize> = (0..cols)
        .map(|col| {
            rows.iter()
                .map(|row| width_of(&plain_text(&row[col])))
                .max()
                .unwrap_or(1)
                .max(1)
        })
        .collect();
    // Cała tabela: `│` + po `" " + cell + " │"` na kolumnę.
    let mut total: usize = 1 + widths.iter().map(|w| w + 3).sum::<usize>();
    while total > width {
        let widest = widths.iter().enumerate().max_by_key(|(_, w)| **w).map(|(i, _)| i).unwrap_or(0);
        if widths[widest] <= 1 {
            break;
        }
        widths[widest] -= 1;
        total -= 1;
    }

    let mut out: Vec<Line> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let mut spans: Vec<Span<'static>> = vec![Span::styled("\u{2502}", frame_style())];
        for (col, cell) in row.iter().enumerate() {
            let base_style = if index == 0 {
                Style::default().fg(FG).add_modifier(Modifier::BOLD)
            } else {
                body()
            };
            let budget = widths[col];
            let mut cell_spans = clip_spans(&inline(&cell, base_style), budget);
            let used: usize = cell_spans.iter().map(|s| width_of(&s.content)).sum();
            spans.push(Span::styled(" ", body()));
            spans.append(&mut cell_spans);
            if used < budget {
                spans.push(Span::styled(spaces(budget - used), body()));
            }
            spans.push(Span::styled(" \u{2502}", frame_style()));
        }
        // Przy bardzo wąskim oknie nawet minimalna siatka nie wchodzi —
        // wtedy przycinamy cały wiersz, zamiast wyjść za ekran.
        out.push(Line::plain(clip_spans(&spans, width)));
        if index == 0 {
            let mut rule = String::from("\u{251C}");
            for (col, w) in widths.iter().enumerate() {
                if col > 0 {
                    rule.push('\u{253C}');
                }
                rule.push_str(&"\u{2500}".repeat(w + 2));
            }
            rule.push('\u{2524}');
            out.push(Line::plain(clip_spans(
                &[Span::styled(rule, frame_style())],
                width,
            )));
        }
    }
    out
}

/// Tekst komórki bez znaczników markdownu — po nim liczy się szerokość kolumny.
fn plain_text(text: &str) -> String {
    inline(text, body())
        .iter()
        .map(|span| span.content.to_string())
        .collect()
}

fn clip_spans(spans: &[Span<'static>], max: usize) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        if used >= max {
            break;
        }
        let text = span.content.to_string();
        let count = width_of(&text);
        if used + count <= max {
            out.push(Span::styled(text, span.style));
            used += count;
            continue;
        }
        let room = max.saturating_sub(used);
        if room == 0 {
            break;
        }
        let keep: String = text.chars().take(room.saturating_sub(1)).collect();
        let mut cut = keep;
        cut.push('\u{2026}');
        out.push(Span::styled(cut, span.style));
        used = max;
    }
    if out.is_empty() {
        out.push(Span::styled(String::new(), body()));
    }
    out
}

// ── Inline ──────────────────────────────────────────────────────────────────

fn inline(text: &str, base: Style) -> Vec<Span<'static>> {
    let segments = inline_segments(text, base, 0);
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (text, style) in segments {
        if text.is_empty() {
            continue;
        }
        if let Some(last) = spans.last_mut() {
            if last.style == style {
                let mut joined = last.content.to_string();
                joined.push_str(&text);
                *last = Span::styled(joined, style);
                continue;
            }
        }
        spans.push(Span::styled(text, style));
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base));
    }
    spans
}

/// Rozbija linię na odcinki (tekst, styl). Niepełne znaczniki zostają tekstem.
fn inline_segments(text: &str, base: Style, depth: usize) -> Vec<(String, Style)> {
    let mut out: Vec<(String, Style)> = Vec::new();
    let mut plain = String::new();
    let mut i = 0usize;
    if depth > 4 {
        return vec![(text.to_string(), base)];
    }
    while i < text.len() {
        let rest = &text[i..];
        // `` `kod` `` — sprawdzany pierwszy, żeby `**` w środku został tekstem.
        if rest.starts_with('`') {
            if let Some(end) = rest[1..].find('`').map(|pos| pos + 1) {
                if end > 1 {
                    flush(&mut out, &mut plain, base);
                    out.push((rest[1..end].to_string(), code_style()));
                    i += end + 1;
                    continue;
                }
            }
        }
        // `![alt](url)` — obraz: sam podpis i adres, bez obrazka.
        if rest.starts_with("![") {
            if let Some((label, url, consumed)) = link_parts(rest, 2) {
                flush(&mut out, &mut plain, base);
                out.push((label, base));
                out.push((format!(" ({})", url), dim()));
                i += consumed;
                continue;
            }
        }
        // `[tekst](url)` — tekst podkreślony + adres w nawiasie, bez klikania.
        if rest.starts_with('[') {
            if let Some((label, url, consumed)) = link_parts(rest, 1) {
                flush(&mut out, &mut plain, base);
                out.push((label, base.add_modifier(Modifier::UNDERLINED)));
                out.push((format!(" ({})", url), dim()));
                i += consumed;
                continue;
            }
        }
        if rest.starts_with("**") {
            if let Some(end) = rest[2..].find("**").map(|pos| pos + 2) {
                if end > 2 {
                    flush(&mut out, &mut plain, base);
                    out.extend(inline_segments(
                        &rest[2..end],
                        base.add_modifier(Modifier::BOLD).fg(FG),
                        depth + 1,
                    ));
                    i += end + 2;
                    continue;
                }
            }
        }
        if rest.starts_with('*') && !rest.starts_with("**") {
            if let Some(end) = rest[1..].find('*').map(|pos| pos + 1) {
                if end > 1 && !rest[end + 1..].starts_with('*') {
                    flush(&mut out, &mut plain, base);
                    out.extend(inline_segments(&rest[1..end], italic(), depth + 1));
                    i += end + 1;
                    continue;
                }
            }
        }
        if rest.starts_with('_') {
            if let Some(end) = rest[1..].find('_').map(|pos| pos + 1) {
                let inner = &rest[1..end];
                let after = rest[end + 1..].chars().next();
                let word_boundary = after.map(|c| !c.is_alphanumeric()).unwrap_or(true);
                if end > 1
                    && !inner.starts_with(' ')
                    && !inner.ends_with(' ')
                    && word_boundary
                {
                    flush(&mut out, &mut plain, base);
                    out.extend(inline_segments(inner, italic(), depth + 1));
                    i += end + 1;
                    continue;
                }
            }
        }
        let ch = rest.chars().next().unwrap_or(' ');
        plain.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut out, &mut plain, base);
    out
}

fn flush(out: &mut Vec<(String, Style)>, plain: &mut String, style: Style) {
    if !plain.is_empty() {
        out.push((std::mem::take(plain), style));
    }
}

/// `[tekst](adres)` → (tekst, adres, zużyte bajty). `open` to `1` dla linku
/// i `2` dla obrazu (`![`).
fn link_parts(text: &str, open: usize) -> Option<(String, String, usize)> {
    let close = text[open..].find("](").map(|pos| pos + open)?;
    let label = text[open..close].to_string();
    let after = &text[close + 2..];
    let end = after.find(')')?;
    let url = after[..end].trim().to_string();
    if label.is_empty() && url.is_empty() {
        return None;
    }
    Some((label, url, close + 2 + end + 1))
}

// ── Zawijanie z zachowaniem stylów ──────────────────────────────────────────

fn wrap_segments(
    segments: Vec<(String, Style)>,
    width: usize,
    first: Vec<Span<'static>>,
    next: Vec<Span<'static>>,
) -> Vec<Line> {
    struct Word {
        text: String,
        style: Style,
    }
    // Słowa bez spacji; spacje wstawiamy sami, więc ciągi spacji w źródle
    // nie rozjeżdżają wiersza.
    let mut words: Vec<Word> = Vec::new();
    for (text, style) in segments {
        let mut buf = String::new();
        for ch in text.chars() {
            if ch == ' ' || ch == '\t' || ch == '\u{00A0}' {
                if !buf.is_empty() {
                    words.push(Word {
                        text: std::mem::take(&mut buf),
                        style,
                    });
                }
            } else {
                buf.push(ch);
            }
        }
        if !buf.is_empty() {
            words.push(Word { text: buf, style });
        }
    }

    let mut out: Vec<Line> = Vec::new();
    let mut spans: Vec<Span<'static>> = first.clone();
    let mut used = 0usize;
    let mut avail = width.saturating_sub(span_width(&first)).max(1);
    let mut empty = true;
    for word in words {
        let word_w = width_of(&word.text);
        if !empty && used > 0 && used + 1 + word_w > avail {
            out.push(Line::plain(spans));
            spans = next.clone();
            avail = width.saturating_sub(span_width(&next)).max(1);
            used = 0;
            empty = true;
        }
        if word_w > avail && used == 0 {
            let chars: Vec<char> = word.text.chars().collect();
            let mut at = 0;
            while at < chars.len() {
                let room = avail.max(1);
                let take = room.min(chars.len() - at);
                let piece: String = chars[at..at + take].iter().collect();
                at += take;
                let piece_w = width_of(&piece);
                let mut line = spans.clone();
                line.push(Span::styled(piece, word.style));
                if at < chars.len() {
                    out.push(Line::plain(line));
                    spans = next.clone();
                    avail = width.saturating_sub(span_width(&next)).max(1);
                    used = 0;
                    empty = true;
                } else {
                    spans = line;
                    used = piece_w;
                    empty = false;
                }
            }
            continue;
        }
        if used > 0 {
            // Sklejamy słowa o tym samym stylu: `` `cargo test` `` ma zostać
            // jednym odcinkiem z tłem, a nie dwoma rozdzielonymi spacją.
            let merged = spans.last().map(|last| last.style == word.style).unwrap_or(false);
            if merged {
                if let Some(last) = spans.last_mut() {
                    let mut joined = last.content.to_string();
                    joined.push(' ');
                    joined.push_str(&word.text);
                    *last = Span::styled(joined, word.style);
                }
            } else {
                spans.push(Span::styled(" ", body()));
                spans.push(Span::styled(word.text, word.style));
            }
            used += 1;
        } else {
            spans.push(Span::styled(word.text, word.style));
        }
        used += word_w;
        empty = false;
    }
    if !empty || out.is_empty() {
        out.push(Line::plain(spans));
    }
    out
}

fn span_width(spans: &[Span<'static>]) -> usize {
    spans.iter().map(|span| width_of(&span.content)).sum()
}

// ── Cache ───────────────────────────────────────────────────────────────────

/// Pamięć ostatnio wyrenderowanych bloków: klucz to (tekst, szerokość).
/// Streaming dopisuje tekst do ostatniego bloku, więc zmienia się tylko jeden
/// klucz — pozostałe bloki są brane z pamięci, bez ponownego parsowania.
pub struct Cache {
    map: RefCell<HashMap<(u64, usize, usize), Rc<Vec<Line>>>>,
    /// Ile razy renderer naprawdę parsował tekst (testy i diagnoza kosztu).
    parses: Cell<usize>,
    hits: Cell<usize>,
}

impl Default for Cache {
    fn default() -> Self {
        Cache::new()
    }
}

impl Cache {
    pub fn new() -> Cache {
        Cache {
            map: RefCell::new(HashMap::new()),
            parses: Cell::new(0),
            hits: Cell::new(0),
        }
    }

    pub fn get(&self, text: &str, width: usize) -> Rc<Vec<Line>> {
        let key = (fingerprint(text), text.len(), width);
        if let Some(hit) = self.map.borrow().get(&key) {
            self.hits.set(self.hits.get() + 1);
            return hit.clone();
        }
        let rendered = Rc::new(render(text, width));
        self.parses.set(self.parses.get() + 1);
        let mut map = self.map.borrow_mut();
        if map.len() >= 64 {
            map.clear();
        }
        map.insert(key, rendered.clone());
        rendered
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn parses(&self) -> usize {
        self.parses.get()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn hits(&self) -> usize {
        self.hits.get()
    }
}

#[cfg(test)]
mod tests;

fn fingerprint(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
