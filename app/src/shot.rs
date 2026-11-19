//! `--screenshot <plik.png>` — zrzut ekranu TUI do PNG.
//!
//! ArndtAi jest programem konsolowym: nie otwiera okna systemowego, wiec
//! „okno" to ekran TUI narysowany do bufora ratatui, a potem zamieniony na
//! piksele (znak = 8×16 px, font 8×8 z `font8x8`). Tryb jest bezobsługowy:
//! nie dotyka terminala, nie zabiera fokusu, nic nie otwiera i kończy się
//! od razu po zapisaniu pliku. Bez tej flagi program działa jak wcześniej.
//!
//! Koder PNG jest minimalny (kolor 8-bit RGB, filtr 0, niezwarte bloki
//! deflate), żeby nie dokładać zależności poza samym fontem.

use crate::tui;
use font8x8::{UnicodeFonts, BASIC_FONTS};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Domyślny kadr zrzutu: 120 kolumn × 38 wierszy = 960×608 px (8×16 px na znak).
pub const DEFAULT_COLS: u16 = 120;
pub const DEFAULT_ROWS: u16 = 38;
const CELL_W: usize = 8;
const CELL_H: usize = 16;
const FRAMES: usize = 120;

/// Ekran sesji o zadanym rozmiarze znakowym (np. `83x38` do porównania
/// linia po linii ze zrzutem referencyjnym).
pub fn save_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let mut app = tui::demo(workspace.clone());
    // Zrzut ma w kadrze pas zadania i odstepy miedzy wpisami, wiec pokazuje
    // poczatek przebiegu; interaktywny start dalej laduje na koncu.
    app.show_scrollback_start();
    let code = render_png(app, path, cols, rows);
    // Workspace zrzutu jest jednorazowy — nie zostawiamy po nim sesji w repo.
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Modal `/model` (runda 14): lista modeli, aktywny model, limity kontekstu.
pub fn save_model_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::model_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Podpowiedzi `/` pod composerem (runda 14): wpisane `/mo`, zaznaczony `/model`.
pub fn save_hints_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::hints_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Modal `/settings` (runda 15): sześć kluczy z `.agent/config`.
pub fn save_settings_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::settings_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Pytanie trybu `ask` (runda 15): composer z `Zezwolić na fs.write …? [t/n]`.
pub fn save_ask_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::ask_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Modal `/skills` (runda 16): dwa skille z workspace'u zrzutu, zaznaczony
/// pierwszy wiersz, znaczniki rozmiaru i „w kontekście".
pub fn save_skills_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::skills_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Scrollback po `/skill` (runda 16): potwierdzenie wczytania treści.
pub fn save_skill_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::skill_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Motyw `jasny` (runda 15): ten sam ekran sesji w drugiej palecie.
pub fn save_light_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::light_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Modal szukania (runda 17): trafienia z numerami i licznikiem.
pub fn save_search_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::search_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Scrollback po `/copy` (runda 17): potwierdzenie OSC 52.
pub fn save_copy_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::copy_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Scrollback po `/export` (runda 17): potwierdzenie zapisu pliku.
pub fn save_export_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::export_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Odpowiedź asystenta w markdownie (runda 18): nagłówek, lista, kod
/// z ramką i podświetlaniem, tabela, cytat.
pub fn save_markdown_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::markdown_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Ten sam ekran w 40 kolumnach — wąska ramka kodu bez prawej krawędzi.
pub fn save_markdown_40_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = PathBuf::from(".agent").join("screenshot-ws");
    let code = render_png(tui::markdown_narrow_shot(workspace.clone()), path, cols, rows);
    let _ = fs::remove_dir_all(&workspace);
    code
}

/// Ekran startowy (lista sesji) na prawdziwych transkryptach z `.agent/sessions`
/// bieżącego workspace'u. Niczego nie tworzy i nie kasuje.
pub fn save_list_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    render_png(tui::list_app(workspace), path, cols, rows)
}

/// Ekran powitalny (`welcome.txt`) — realna ścieżka, model i tryb zgód.
pub fn save_welcome_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    render_png(tui::welcome_app(workspace), path, cols, rows)
}

/// Modal „Resume session" (`resume-modal.txt`) na prawdziwych transkryptach
/// z `.agent/sessions`; nic nie zapisuje i nie kasuje.
pub fn save_resume_at(path: &str, cols: u16, rows: u16) -> i32 {
    let workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    render_png(tui::resume_app(workspace), path, cols, rows)
}

fn render_png(app: tui::App, path: &str, cols: u16, rows: u16) -> i32 {
    let backend = TestBackend::new(cols, rows);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("nie mozna narysowac ekranu: {}", e);
            return 2;
        }
    };

    // 120 klatek: kursor i spinner mrugaja, wiec obrazek jest „zywy", a nie pusty.
    let mut app = app;
    for _ in 0..FRAMES {
        let _ = terminal.draw(|frame| app.draw(frame));
        app.tick();
    }

    let canvas = rasterize(terminal.backend().buffer(), cols, rows);
    let out = resolve_out(path);
    if let Some(dir) = out.parent() {
        if !dir.as_os_str().is_empty() {
            let _ = fs::create_dir_all(dir);
        }
    }
    match canvas.write_png(&out) {
        Ok(()) => {
            println!("zapisano zrzut: {}", out.display());
            0
        }
        Err(e) => {
            eprintln!("nie mozna zapisac zrzutu {}: {}", out.display(), e);
            2
        }
    }
}

/// Ścieżka `app/shots/x.png` podana z katalogu `app/` trafia do `app/shots/x.png`
/// (a nie `app/app/shots/x.png`) — dokumentacja i bramki używają obu form.
fn resolve_out(path: &str) -> PathBuf {
    let given = PathBuf::from(path);
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.file_name().and_then(|n| n.to_str()) == Some("app") {
            if let Ok(rest) = given.strip_prefix("app") {
                if rest.components().count() > 1 {
                    return cwd.join(rest);
                }
            }
        }
    }
    given
}

// --- Bufor znakow -> piksele ------------------------------------------------

fn rasterize(buffer: &Buffer, cols: u16, rows: u16) -> Canvas {
    let width = cols as usize * CELL_W;
    let height = rows as usize * CELL_H;
    let mut canvas = Canvas::new(width, height);
    let cols = cols as usize;
    for (index, cell) in buffer.content.iter().enumerate() {
        let cx = index % cols;
        let cy = index / cols;
        let x = cx * CELL_W;
        let y = cy * CELL_H;
        canvas.rect(x, y, CELL_W, CELL_H, rgb_bg(cell.bg));
        let symbol = cell.symbol();
        let raw = symbol.chars().next().unwrap_or(' ');
        // Braille i ramki wypełniają całą komórkę 8×16 — inaczej logo i boki
        // panelu wyglądają na mniejsze i przerywane niż w terminalu.
        if let Some(glyph) = braille_glyph(raw).or_else(|| box_glyph(raw)) {
            rgba_glyph16(&mut canvas, x, y, &glyph, rgb_fg(cell.fg));
        } else if let Some(glyph) = custom_glyph(raw).or_else(|| BASIC_FONTS.get(fallback(raw))) {
            rgba_glyph(&mut canvas, x, y + 4, &glyph, rgb_fg(cell.fg));
        }
    }
    canvas
}

/// Braille w pełnej komórce 8×16: 2 kolumny × 4 rzędy punktów, punkt 3×3 px.
/// Wcześniej punkt miał 2×2 px w połowie wysokości, przez co logo w PNG było
/// niższe i grubsze niż ten sam znak w terminalu.
fn braille_glyph(ch: char) -> Option<[u8; 16]> {
    if !('\u{2800}'..='\u{28FF}').contains(&ch) {
        return None;
    }
    let bits = ch as u32 - 0x2800;
    let cells = [
        (0usize, 0usize),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    let mut g = [0u8; 16];
    for (i, (cx, cy)) in cells.iter().enumerate() {
        if bits & (1 << i) != 0 {
            for dy in 0..3 {
                for dx in 0..3 {
                    g[cy * 4 + dy] |= 1 << (cx * 4 + dx);
                }
            }
        }
    }
    Some(g)
}

fn hline(g: &mut [u8; 16], y: usize, from: usize, to: usize) {
    for x in from..=to {
        g[y] |= 1 << x;
    }
}

fn vline(g: &mut [u8; 16], x0: usize, x1: usize, from: usize, to: usize) {
    for y in from..=to {
        g[y] |= (1 << x0) | (1 << x1);
    }
}

/// Ramki w pełnej wysokości komórki: `│` nie może być kreskowane, a narożniki
/// `┌ ┐ └ ┘` i trójniki `├ ┤` muszą domykać panel (modal ma pełną ramkę).
fn box_glyph(ch: char) -> Option<[u8; 16]> {
    let mut g = [0u8; 16];
    match ch {
        '\u{2500}' | '\u{2501}' => {
            hline(&mut g, 7, 0, 7);
            hline(&mut g, 8, 0, 7);
        }
        '\u{2502}' | '\u{2503}' => vline(&mut g, 3, 4, 0, 15),
        '\u{250C}' => {
            hline(&mut g, 7, 3, 7);
            hline(&mut g, 8, 4, 7);
            vline(&mut g, 3, 4, 7, 15);
        }
        '\u{2510}' => {
            hline(&mut g, 7, 0, 4);
            hline(&mut g, 8, 0, 3);
            vline(&mut g, 3, 4, 7, 15);
        }
        '\u{2514}' => {
            hline(&mut g, 7, 3, 7);
            hline(&mut g, 8, 4, 7);
            vline(&mut g, 3, 4, 0, 8);
        }
        '\u{2518}' => {
            hline(&mut g, 7, 0, 4);
            hline(&mut g, 8, 0, 3);
            vline(&mut g, 3, 4, 0, 8);
        }
        '\u{251C}' => {
            vline(&mut g, 3, 4, 0, 15);
            hline(&mut g, 7, 3, 7);
            hline(&mut g, 8, 4, 7);
        }
        '\u{2524}' => {
            vline(&mut g, 3, 4, 0, 15);
            hline(&mut g, 7, 0, 4);
            hline(&mut g, 8, 0, 3);
        }
        // Zaokrąglone narożniki composera: ten sam bok, tylko o piksel krótszy
        // (przy 8×16 px łuk jest umowny, ale nie może przerywać ramki).
        '\u{256D}' => {
            hline(&mut g, 7, 4, 7);
            hline(&mut g, 8, 4, 7);
            vline(&mut g, 3, 4, 8, 15);
        }
        '\u{256E}' => {
            hline(&mut g, 7, 0, 3);
            hline(&mut g, 8, 0, 3);
            vline(&mut g, 3, 4, 8, 15);
        }
        '\u{2570}' => {
            hline(&mut g, 7, 4, 7);
            hline(&mut g, 8, 4, 7);
            vline(&mut g, 3, 4, 0, 7);
        }
        '\u{256F}' => {
            hline(&mut g, 7, 0, 3);
            hline(&mut g, 8, 0, 3);
            vline(&mut g, 3, 4, 0, 7);
        }
        _ => return None,
    }
    Some(g)
}

/// Znaki chrome'u (akcent, spinner), których nie ma w foncie 8×8.
/// Bit 0 w bajcie to lewa kolumna — tak samo jak w [`rgba_glyph`].
fn custom_glyph(ch: char) -> Option<[u8; 8]> {
    match ch {
        '\u{2759}' => Some([0x3c; 8]), // ❙ gruby akcent
        '\u{276F}' => {
            let mut g = [0u8; 8];
            for y in 0..8usize {
                let d = if y < 4 { y } else { 7 - y };
                let x = 1 + d;
                g[y] |= 1 << x;
                g[y] |= 1 << (x + 1);
            }
            Some(g)
        }
        '\u{25C7}' => {
            let mut g = [0u8; 8];
            let pts = [
                (3, 0),
                (4, 0),
                (2, 1),
                (5, 1),
                (1, 2),
                (6, 2),
                (0, 3),
                (7, 3),
                (0, 4),
                (7, 4),
                (1, 5),
                (6, 5),
                (2, 6),
                (5, 6),
                (3, 7),
                (4, 7),
            ];
            for (x, y) in pts {
                g[y] |= 1 << x;
            }
            Some(g)
        }
        '\u{25C6}' => {
            let mut g = [0u8; 8];
            for y in 0..8usize {
                let d = if y < 4 { y } else { 7 - y };
                let (from, to) = (3usize.saturating_sub(d), 4 + d);
                for x in from..=to {
                    g[y] |= 1 << x;
                }
            }
            Some(g)
        }
        // Reguła działu `——` (short_rule markdownu): kreska w połowie
        // wysokości komórki; bez tego PNG gubił całą linię.
        '\u{2014}' | '\u{2013}' => {
            let mut g = [0u8; 8];
            g[4] = 0xff;
            Some(g)
        }
        '\u{2713}' => {
            let mut g = [0u8; 8];
            for (x, y) in [
                (1, 4),
                (2, 5),
                (3, 6),
                (4, 5),
                (5, 4),
                (6, 3),
                (7, 2),
                (2, 4),
                (3, 5),
                (4, 4),
                (5, 3),
                (6, 2),
            ] {
                g[y] |= 1 << x;
            }
            Some(g)
        }
        '\u{2717}' => {
            let mut g = [0u8; 8];
            for i in 0..6usize {
                g[1 + i] |= 1 << (1 + i);
                g[1 + i] |= 1 << (6 - i);
            }
            Some(g)
        }
        '\u{2026}' => Some([0, 0, 0, 0, 0, 0, 0x92, 0]), // …
        // Strzałki z stopki modala: `↑/↓:navigate` — bez nich PNG gubi klawisz.
        '\u{2191}' => Some(arrow(true)),
        '\u{2193}' => Some(arrow(false)),
        '\u{00B7}' | '\u{2022}' | '\u{22C5}' => Some([0, 0, 0, 0, 0x18, 0, 0, 0]),
        _ => None,
    }
}

/// Strzałka 8×8: trzon w kolumnach 3–4, grot u góry (`up`) albo u dołu.
fn arrow(up: bool) -> [u8; 8] {
    let mut g = [0u8; 8];
    for y in 2..8 {
        g[y] |= 0b0001_1000;
    }
    let head = [0x18u8, 0x3c, 0x7e];
    for (i, bits) in head.iter().enumerate() {
        if up {
            g[i] |= *bits;
        } else {
            g[7 - i] |= *bits;
        }
    }
    g
}

/// Rysuje 16-wierszowy wzór (braille, ramki); bit 0 w bajcie to lewa kolumna.
fn rgba_glyph16(canvas: &mut Canvas, x: usize, y: usize, glyph: &[u8; 16], color: [u8; 3]) {
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..8usize {
            if bits & (1 << col) != 0 {
                canvas.pixel(x + col, y + row, color);
            }
        }
    }
}

/// Rysuje 8×8 znak; bit 0 w bajcie to lewa kolumna.
fn rgba_glyph(canvas: &mut Canvas, x: usize, y: usize, glyph: &[u8; 8], color: [u8; 3]) {
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..8usize {
            if bits & (1 << col) != 0 {
                canvas.pixel(x + col, y + row, color);
            }
        }
    }
}

/// Znaki, ktorych nie ma w foncie 8×8, zamieniamy na czytelny odpowiednik ASCII.
fn fallback(ch: char) -> char {
    match ch {
        '❯' | '›' | '»' | '→' | '▸' => '>',
        // Cudzysłowy i nawiasy kątowe: w PNG tylko kreska, żeby nie zostało
        // puste pole.
        '‹' | '«' | '←' | '◂' => '<',
        '❙' | '│' | '┃' => '|',
        '─' | '━' => '-',
        '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼' => '+',
        '✓' | '✔' => 'v',
        '✗' | '✘' | '×' => 'x',
        '◆' | '◈' | '▪' | '■' | '●' => '*',
        '◇' | '○' | '◦' => 'o',
        '·' | '•' => '.',
        '…' => '.',
        '⠋' | '⠙' | '⠹' | '⠸' | '⠼' | '⠴' | '⠦' | '⠧' | '⠇' | '⠏' => '*',
        '↑' => '^',
        '↓' => 'v',
        // Polskie znaki diakrytyczne (BASIC_FONTS ich nie ma) → litera bez
        // ogonka. Bez tego `Zezwolic` w zrzucie miałby dziurę.
        'ą' => 'a',
        'ć' => 'c',
        'ę' => 'e',
        'ł' => 'l',
        'ń' => 'n',
        'ó' => 'o',
        'ś' => 's',
        'ź' | 'ż' => 'z',
        'Ą' => 'A',
        'Ć' => 'C',
        'Ę' => 'E',
        'Ł' => 'L',
        'Ń' => 'N',
        'Ó' => 'O',
        'Ś' => 'S',
        'Ź' | 'Ż' => 'Z',
        other if other.is_ascii() => other,
        _ => ' ',
    }
}

fn rgb_fg(color: Color) -> [u8; 3] {
    if color == Color::Reset {
        return [225, 225, 225];
    }
    rgb(color)
}

fn rgb_bg(color: Color) -> [u8; 3] {
    if color == Color::Reset {
        return [12, 12, 12];
    }
    rgb(color)
}

fn rgb(color: Color) -> [u8; 3] {
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Reset => [10, 10, 10],
        Color::Black => [12, 12, 12],
        Color::Red => [247, 118, 142],
        Color::Green => [158, 206, 106],
        Color::Yellow => [224, 175, 104],
        Color::Blue => [122, 162, 247],
        Color::Magenta => [187, 154, 247],
        Color::Cyan => [125, 207, 255],
        Color::Gray => [160, 160, 160],
        Color::DarkGray => [80, 80, 80],
        Color::LightRed | Color::LightMagenta => [255, 150, 190],
        Color::LightGreen => [180, 230, 140],
        Color::LightYellow => [240, 200, 140],
        Color::LightBlue | Color::LightCyan => [170, 220, 255],
        Color::White => [240, 240, 240],
        Color::Indexed(i) => {
            if i >= 232 {
                let v = 8 + (i - 232) * 10;
                [v, v, v]
            } else {
                [200, 200, 200]
            }
        }
    }
}

// --- Canvas + PNG -----------------------------------------------------------

struct Canvas {
    w: usize,
    h: usize,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![0; w * h * 3],
        }
    }

    fn pixel(&mut self, x: usize, y: usize, c: [u8; 3]) {
        if x >= self.w || y >= self.h {
            return;
        }
        let i = (y * self.w + x) * 3;
        self.px[i] = c[0];
        self.px[i + 1] = c[1];
        self.px[i + 2] = c[2];
    }

    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, c: [u8; 3]) {
        for yy in y..(y + h).min(self.h) {
            for xx in x..(x + w).min(self.w) {
                self.pixel(xx, yy, c);
            }
        }
    }

    fn write_png(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        // Surowe skanline: filtr 0 + RGB.
        let mut raw = Vec::with_capacity((self.w * 3 + 1) * self.h);
        for y in 0..self.h {
            raw.push(0u8);
            raw.extend_from_slice(&self.px[y * self.w * 3..(y + 1) * self.w * 3]);
        }

        let mut out = Vec::new();
        out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&(self.w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(self.h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bitow, RGB, brak przeplotu
        write_chunk(&mut out, b"IHDR", &ihdr);
        write_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
        write_chunk(&mut out, b"IEND", &[]);

        fs::write(path, out)
    }
}

fn write_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Strumien zlib z niezwartymi blokami deflate (bez zaleznosci).
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 65_535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]);
    let mut chunks = raw.chunks(65_535).peekable();
    if chunks.peek().is_none() {
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0xff, 0xff]);
    }
    while let Some(chunk) = chunks.next() {
        let last = chunks.peek().is_none();
        out.push(if last { 0x01 } else { 0x00 });
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in data {
        a = (a + *byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_is_a_non_blank_png() {
        let _guard = crate::tui::test_guard();
        let dir = std::env::temp_dir().join(format!("arndtai-shot-{}", std::process::id()));
        let path = dir.join("now.png");
        let code = save_at(path.to_str().unwrap(), DEFAULT_COLS, DEFAULT_ROWS);
        assert_eq!(code, 0);
        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        assert!(bytes.len() > 10_000, "zrzut nie moze byc pusty");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn glyph_pixels_land_in_the_cell() {
        // 'A' z fontu 8x8 musi zapalic piksele — inaczej zrzut jest pusty.
        let mut buffer = Buffer::empty(ratatui::layout::Rect::new(
            0,
            0,
            DEFAULT_COLS,
            DEFAULT_ROWS,
        ));
        buffer.set_string(
            1,
            1,
            "ArndtAi test",
            ratatui::style::Style::default().fg(ratatui::style::Color::White),
        );
        let canvas = rasterize(&buffer, DEFAULT_COLS, DEFAULT_ROWS);
        let first = canvas.px[..3].to_vec();
        assert!(canvas.px.chunks(3).any(|p| p != first.as_slice()));
    }

    #[test]
    fn chrome_glyphs_are_drawn_by_hand() {
        // Akcenty i spinner nie są w BASIC_FONTS — muszą mieć własny wzór.
        for ch in [
            '\u{2759}', '\u{276F}', '\u{2717}', '\u{25C6}', '\u{25C7}', '\u{2014}',
        ] {
            let glyph = custom_glyph(ch).unwrap_or_else(|| panic!("brak wzoru dla {:?}", ch));
            assert!(
                glyph.iter().any(|row| *row != 0),
                "wzór {:?} nie moze byc pusty",
                ch
            );
        }
        assert!(custom_glyph('A').is_none(), "litery idą z fontu 8x8");
    }

    /// Ramki i braille rysujemy w pełnej komórce 8×16: `│` musi przechodzić
    /// przez cały wiersz (nie kreskować), a narożniki muszą domykać panel.
    #[test]
    fn box_and_braille_glyphs_fill_the_whole_cell() {
        for ch in [
            '\u{250C}', '\u{2510}', '\u{2514}', '\u{2518}', '\u{251C}', '\u{2524}',
        ] {
            let glyph = box_glyph(ch).unwrap_or_else(|| panic!("brak wzoru dla {:?}", ch));
            assert!(glyph[0] != 0 || glyph[15] != 0, "narożnik {:?} musi mieć bok", ch);
        }
        let bar = box_glyph('\u{2502}').unwrap();
        assert!(bar.iter().all(|row| *row != 0), "`│` musi być ciągłe");
        let braille = braille_glyph('\u{28FF}').unwrap();
        assert!(braille[0] != 0 && braille[14] != 0, "braille ma wypełniać komórkę");
        assert!(braille_glyph('A').is_none());
    }

    /// Polskie znaki i nawiasy kątowe w zrzucie nie mogą zostawiać dziur:
    /// `BASIC_FONTS` ich nie ma, więc `fallback` transliteruje je na ASCII.
    #[test]
    fn polish_letters_and_angle_quotes_have_a_glyph_in_the_png() {
        for (ch, expected) in [
            ('ć', 'c'),
            ('ą', 'a'),
            ('ę', 'e'),
            ('ł', 'l'),
            ('ó', 'o'),
            ('ż', 'z'),
            ('‹', '<'),
            ('›', '>'),
        ] {
            let mapped = fallback(ch);
            assert_eq!(mapped, expected, "{:?} -> {:?}", ch, mapped);
            assert!(
                crate::shot::custom_glyph(mapped).is_some() || font8x8::BASIC_FONTS.get(mapped).is_some(),
                "{:?} nie ma wzoru w foncie",
                mapped
            );
        }
        // Pytanie trybu `ask` po transliteracji czyta się bez dziur.
        let question: String = "Zezwolić na fs.write".chars().map(fallback).collect();
        assert_eq!(question, "Zezwolic na fs.write");
    }

}
