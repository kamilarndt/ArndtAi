//! Ekran powitalny i modal „Resume session" — geometria wzięta ze zrzutów
//! `reference/grok-cli-screens/welcome.txt` (83 kolumny) i `resume-modal.txt`
//! (85 kolumn). To realny render Grok Build 1.0.77, więc traktujemy go jak
//! źródło prawdy o układzie: wcięcia, wyrównania i znaki są liczone tak, żeby
//! przy szerokości referencyjnej wypaść znak po znaku, a przy innych
//! szerokościach zwijać się bez wychodzenia za krawędź.
//!
//! Moduł nie rysuje — buduje wiersze (znak + styl) o zadanej szerokości.
//! `tui.rs` przenosi je do bufora ratatui, a testy sprawdzają sam układ,
//! bez terminala.

use crate::sessions::{format_age, Filter, SessionRow};
use crate::tui::{BG, FG, FG_DARK, GRAY, GRAY_DIM, GRAY_BRIGHT, GREEN, PROMPT_BORDER, YELLOW};
use ratatui::style::Style;

// ── Metryki ze zrzutów ──────────────────────────────────────────────────────

/// Lewy margines treści: ścieżka, ramka composera i linia wersji zaczynają się
/// w kolumnie 2 (zrzut: `"  ╭──…"`, `"  │ ❯ …"`).
const CONTENT_LEFT: u16 = 2;
/// Szerokość bloku menu: w zrzucie etykieta stoi w kolumnie 17, a skrót kończy
/// się w kolumnie 68, czyli blok ma 51 kolumn wyśrodkowanych w treści.
const MENU_WIDTH: u16 = 51;
/// Logo zajmuje 14 kolumn i jest wyśrodkowane (35 + 14 = 49 w zrzucie).
const LOGO_WIDTH: u16 = 14;
/// Logo z oryginału — kształt i wcięcia przepisane ze zrzutu, nie rysowane od
/// nowa. Prowadzące `⠀` (U+2800) to celowe wcięcie wewnątrz klocka.
pub(crate) const LOGO: [&str; 7] = [
    "\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{28C0}\u{28C0}\u{2840}\u{2800}\u{2800}\u{2800}\u{2880}\u{2804}",
    "\u{2800}\u{2800}\u{2800}\u{28E0}\u{28FE}\u{283F}\u{281B}\u{281B}\u{281B}\u{281B}\u{2880}\u{2874}\u{2801}\u{2800}",
    "\u{2800}\u{2800}\u{28FC}\u{285F}\u{2801}\u{2800}\u{2800}\u{2800}\u{2880}\u{2874}\u{283B}\u{28FF}\u{2840}\u{2800}",
    "\u{2800}\u{2800}\u{28FF}\u{2847}\u{2800}\u{2800}\u{2800}\u{2814}\u{2801}\u{2800}\u{2800}\u{28FF}\u{2847}\u{2800}",
    "\u{2800}\u{2800}\u{28B9}\u{28F7}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2880}\u{28F4}\u{287F}\u{2800}\u{2800}",
    "\u{2800}\u{2880}\u{281E}\u{2801}\u{2820}\u{28B6}\u{28F6}\u{28F6}\u{28F6}\u{283F}\u{280B}\u{2800}\u{2800}\u{2800}",
    "\u{2810}\u{2801}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}\u{2800}",
];

/// Pozycje menu — etykiety i skróty 1:1 ze zrzutu (kolejność i brak skrótu
/// przy `Changelog` też).
pub(crate) const MENU: [(&str, &str); 4] = [
    ("Resume session", "ctrl+r"),
    ("New worktree", "ctrl+w"),
    ("Changelog", ""),
    ("Quit", "ctrl+q"),
];

/// Stopka modala — dokładnie te separatory `  │  ` i te etykiety co w zrzucie.
const RESUME_FOOTER: [&str; 5] = [
    "Esc:back",
    "Enter:select",
    "ctrl+w:worktree",
    "\u{2191}/\u{2193}:navigate",
    "f:filter",
];
const FOOTER_SEPARATOR: &str = "  \u{2502}  ";

// ── Wiersz: znaki + style + wyrównania ──────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub style: Style,
}

/// Jeden wiersz ekranu o stałej szerokości. Pozycje są liczone od kolumny 0,
/// tak jak w zrzucie, więc test może porównać tekst wiersza wprost.
pub struct Row {
    cells: Vec<Cell>,
}

impl Row {
    fn blank(width: u16, style: Style) -> Row {
        Row {
            cells: vec![
                Cell {
                    ch: ' ',
                    style,
                };
                width as usize
            ],
        }
    }

    #[cfg(test)]
    pub fn text(&self) -> String {
        self.cells.iter().map(|c| c.ch).collect()
    }

    /// Wpisuje tekst od kolumny `x`; nadmiar (poza szerokością) jest ucinany.
    fn put(&mut self, x: u16, text: &str, style: Style) {
        let start = x as usize;
        for (i, ch) in text.chars().enumerate() {
            let at = start + i;
            if at >= self.cells.len() {
                return;
            }
            self.cells[at] = Cell { ch, style };
        }
    }

    /// Wpisuje tekst tak, żeby jego ostatni znak wypadł w kolumnie `right-1`.
    fn right(&mut self, right: u16, text: &str, style: Style) {
        let len = text.chars().count() as u16;
        if len <= right {
            self.put(right - len, text, style);
        }
    }

    /// Maluje tło (pasek zaznaczenia) w kolumnach `x0..x1`.
    fn fill(&mut self, x0: u16, x1: u16, style: Style) {
        for x in x0..x1.min(self.cells.len() as u16) {
            self.cells[x as usize].style = style;
        }
    }

    pub fn spans(&self) -> Vec<(String, Style)> {
        let mut spans: Vec<(String, Style)> = Vec::new();
        for cell in &self.cells {
            match spans.last_mut() {
                Some((text, style)) if *style == cell.style => text.push(cell.ch),
                _ => spans.push((cell.ch.to_string(), cell.style)),
            }
        }
        spans
    }
}

fn text_style() -> Style {
    Style::default().fg(FG).bg(BG)
}

fn dim_style() -> Style {
    Style::default().fg(GRAY).bg(BG)
}

fn body_style() -> Style {
    Style::default().fg(FG_DARK).bg(BG)
}

fn border_style() -> Style {
    Style::default().fg(PROMPT_BORDER).bg(BG)
}

/// Ścieżka workspace'u skrócona do `~/…` — tak jak w nagłówku zrzutu.
pub fn short_path(path: &std::path::Path) -> String {
    let full = path.to_string_lossy().to_string();
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            if let Some(rest) = full.strip_prefix(&home) {
                let rest = rest.trim_start_matches('/');
                return if rest.is_empty() {
                    "~".to_string()
                } else {
                    format!("~/{}", rest)
                };
            }
        }
    }
    full
}

// ── Ekran powitalny ─────────────────────────────────────────────────────────

pub struct WelcomeState {
    /// Ścieżka workspace'u już skrócona (`~/projects/ArndtAi`).
    pub path: String,
    /// Realny model z konfiguracji (`mimo-v2.6-flash`, `offline (mock)`).
    pub model: String,
    /// Tryb zgód z allowlisty narzędzi (`always-approve` / `ask`).
    pub mode: String,
    /// Wersja z `Cargo.toml`.
    pub version: String,
    pub announcements: Vec<String>,
    pub tip: String,
    /// Jednolinijkowy komunikat (np. po wybraniu `Changelog`).
    pub note: Option<String>,
}

pub(crate) struct WelcomeGeometry {
    pub logo_x: u16,
    pub menu_x: u16,
    pub menu_w: u16,
    pub composer_w: u16,
}

pub(crate) fn welcome_geometry(width: u16) -> WelcomeGeometry {
    let width = width.max(1);
    let content_w = width.saturating_sub(CONTENT_LEFT);
    let logo_w = content_w.min(LOGO_WIDTH);
    let menu_w = content_w.min(MENU_WIDTH);
    WelcomeGeometry {
        logo_x: CONTENT_LEFT + content_w.saturating_sub(logo_w) / 2,
        menu_x: CONTENT_LEFT + content_w.saturating_sub(menu_w) / 2,
        menu_w,
        composer_w: content_w,
    }
}

/// Wiersze ekranu powitalnego. Liczba wierszy jest stała (19), chyba że ekran
/// jest niższy — wtedy końcówka jest ucinana, a nie rozjeżdżana.
pub fn welcome(state: &WelcomeState, width: u16, height: u16) -> Vec<Row> {
    let width = width.max(1);
    let geo = welcome_geometry(width);
    let blank = Style::default().bg(BG);
    let mut rows: Vec<Row> = Vec::new();

    // 1. Nagłówek: dwie kolumny oddechu + skrócona ścieżka (przygaszona).
    let mut header = Row::blank(width, blank);
    header.put(CONTENT_LEFT, &state.path, dim_style());
    rows.push(header);

    // 2. Logo: wyśrodkowane w kolumnie treści, bez własnej ramki.
    for line in LOGO.iter() {
        let mut row = Row::blank(width, blank);
        row.put(geo.logo_x, line, body_style());
        rows.push(row);
    }

    // 3. Menu: etykieta z lewej, skrót wyrównany do prawej krawędzi bloku.
    for (label, key) in MENU.iter() {
        let mut row = Row::blank(width, blank);
        row.put(geo.menu_x, label, text_style());
        if !key.is_empty() {
            row.right(geo.menu_x + geo.menu_w, key, dim_style());
        }
        rows.push(row);
    }

    // 4. Ogłoszenia w tym samym wcięciu co etykiety menu, `Tip:` o kolumnę
    //    dalej (zrzut: `   Tip: …` przy `Resume session` w kolumnie 17).
    for line in state.announcements.iter() {
        let mut row = Row::blank(width, blank);
        row.put(geo.menu_x, line, dim_style());
        rows.push(row);
    }
    let mut tip = Row::blank(width, blank);
    tip.put(CONTENT_LEFT + 1, &state.tip, Style::default().fg(GRAY_DIM).bg(BG));
    rows.push(tip);

    // 5. Composer: zaokrąglona ramka, `❯` w środku, etykieta w dolnej krawędzi.
    let composer: Vec<Row> = composer_rows(&state.model, &state.mode, width);
    rows.extend(composer);

    // 6. Pod ramką, wyrównana do prawej, linia wersji.
    let version = format!("ArndtAi {}", state.version);
    let mut last = Row::blank(width, blank);
    last.right(width, &version, dim_style());
    rows.push(last);

    // 7. Jednolinijkowa notka (np. `Changelog`) — tylko gdy jest miejsce.
    if let Some(note) = &state.note {
        let mut row = Row::blank(width, blank);
        row.put(CONTENT_LEFT + 1, note, Style::default().fg(YELLOW).bg(BG));
        rows.push(row);
    }

    rows.truncate(height as usize);
    rows
}

/// Trzy wiersze composera: górna krawędź, linia z `❯`, dolna krawędź z
/// etykietą wpisaną w nią i wyrównaną do prawej (`╰─…─ <etykieta> ─╯`).
fn composer_rows(model: &str, mode: &str, width: u16) -> Vec<Row> {
    let geo = welcome_geometry(width);
    let w = geo.composer_w;
    let blank = Style::default().bg(BG);
    let border = border_style();
    let mut rows = Vec::new();
    if w < 2 {
        return rows;
    }

    let mut top = Row::blank(width, blank);
    top.put(CONTENT_LEFT, "\u{256D}", border);
    for x in 1..w - 1 {
        top.put(CONTENT_LEFT + x, "\u{2500}", border);
    }
    top.put(CONTENT_LEFT + w - 1, "\u{256E}", border);
    rows.push(top);

    let mut mid = Row::blank(width, blank);
    mid.put(CONTENT_LEFT, "\u{2502}", border);
    // Zrzut: `│ ❯` — kursor o kolumnę od ramki, nie pod nią.
    mid.put(CONTENT_LEFT + 2, "\u{276F}", border);
    mid.put(CONTENT_LEFT + w - 1, "\u{2502}", border);
    rows.push(mid);

    // Etykieta: `<model> · <tryb>`, w dolnej krawędzi, do prawej. Miejsce na
    // etykietę to szerokość ramki minus `╰`, `╯`, dwie spacje i kreska.
    let label = format!("{} \u{00B7} {}", model, mode);
    let room = (w as usize).saturating_sub(6);
    let label: String = label.chars().take(room).collect();
    let inner = (w as usize)
        .saturating_sub(5)
        .saturating_sub(label.chars().count());
    let mut bottom = Row::blank(width, blank);
    bottom.put(CONTENT_LEFT, "\u{2570}", border);
    for x in 1..=inner as u16 {
        bottom.put(CONTENT_LEFT + x, "\u{2500}", border);
    }
    let label_x = CONTENT_LEFT + 1 + inner as u16 + 1;
    bottom.put(label_x, &label, Style::default().fg(GRAY_BRIGHT).bg(BG));
    let tail = label_x + label.chars().count() as u16;
    bottom.put(tail + 1, "\u{2500}", border);
    bottom.put(tail + 2, "\u{256F}", border);
    rows.push(bottom);

    rows
}

// ── Modal „Resume session" ──────────────────────────────────────────────────

pub struct ResumeState<'a> {
    /// Ścieżka workspace'u już skrócona — nagłówek nad panelem.
    pub path: String,
    /// Sesje po filtrze i szukaniu (kolejność z `sessions::scan`).
    pub sessions: &'a [SessionRow],
    pub selected: usize,
    pub offset: usize,
    pub query: String,
    pub searching: bool,
    pub filter: Filter,
    pub now: u64,
    pub note: Option<String>,
}

/// Wiersze modala. Panel zajmuje całą szerokość i wszystko poniżej nagłówka
/// ze ścieżką (zrzut: 85 kolumn, 52 wiersze panelu pod ścieżką).
pub fn resume(state: &ResumeState, width: u16, height: u16) -> Vec<Row> {
    let width = width.max(1);
    let blank = Style::default().bg(BG);
    let border = border_style();
    let mut rows: Vec<Row> = Vec::new();

    let mut header = Row::blank(width, blank);
    header.put(CONTENT_LEFT, &state.path, dim_style());
    rows.push(header);

    let panel_h = height.saturating_sub(1).max(3);
    // Ramki 2 + tytuł, separator, pasek narzędzi, linia pod nim i stopka = 7.
    let reserved = 7u16;
    let list_h = panel_h.saturating_sub(reserved);

    rows.push(edge(width, '\u{250C}', '\u{2510}', border));

    // Pasek tytułu: ` Resume session` po lewej, `[✗]` przy samej ramce.
    let mut title = Row::blank(width, blank);
    title.put(0, "\u{2502}", border);
    title.put(width.saturating_sub(1), "\u{2502}", border);
    title.put(CONTENT_LEFT + 1, "Resume session", text_style());
    title.right(width.saturating_sub(1), "[\u{2717}]", dim_style());
    rows.push(title);

    rows.push(edge(width, '\u{251C}', '\u{2524}', border));

    // Pasek narzędzi: `/ to search` (albo wpisany tekst) po lewej,
    // `<n> sessions` po prawej — koniec tekstu dwie kolumny od ramki.
    let mut tools = Row::blank(width, blank);
    tools.put(0, "\u{2502}", border);
    tools.put(width.saturating_sub(1), "\u{2502}", border);
    let left = if state.searching || !state.query.is_empty() {
        format!("/ {}", state.query)
    } else {
        "/ to search".to_string()
    };
    tools.put(CONTENT_LEFT, &left, dim_style());
    if state.filter != Filter::All {
        // Filtr musi być widoczny, choć zrzut go nie pokazuje (0 sesji = All).
        let at = CONTENT_LEFT + left.chars().count() as u16 + 2;
        tools.put(
            at,
            &format!("\u{00B7} {}", state.filter.label()),
            Style::default().fg(GRAY_DIM).bg(BG),
        );
    }
    let count = format!("{} sessions", state.sessions.len());
    tools.right(width.saturating_sub(3), &count, dim_style());
    rows.push(tools);

    // Linia pod paskiem narzędzi — kreski między bokami ramki, jak w zrzucie.
    let mut rule = Row::blank(width, blank);
    rule.put(0, "\u{2502}", border);
    rule.put(width.saturating_sub(1), "\u{2502}", border);
    for x in 1..width.saturating_sub(1) {
        rule.put(x, "\u{2500}", border);
    }
    rows.push(rule);

    if state.sessions.is_empty() {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        row.put(CONTENT_LEFT + 1, "No matches", dim_style());
        rows.push(row);
    } else {
        let first = state.offset.min(state.sessions.len().saturating_sub(1));
        let visible = state.sessions[first..]
            .iter()
            .take(list_h as usize)
            .enumerate();
        for (i, session) in visible {
            let index = first + i;
            let mut row = Row::blank(width, blank);
            row.put(0, "\u{2502}", border);
            row.put(width.saturating_sub(1), "\u{2502}", border);
            if index == state.selected {
                row.fill(1, width.saturating_sub(1), Style::default().bg(crate::tui::BG_LIGHT));
            }
            let base = if index == state.selected {
                Style::default().fg(FG).bg(crate::tui::BG_LIGHT)
            } else {
                text_style()
            };
            let dim = if index == state.selected {
                Style::default().fg(GRAY).bg(crate::tui::BG_LIGHT)
            } else {
                dim_style()
            };
            let age = format_age(session.age_secs(state.now));
            row.right(width.saturating_sub(3), &age, dim);
            let room = (width as usize)
                .saturating_sub(CONTENT_LEFT as usize + 2 + age.chars().count() + 1)
                .max(4);
            let mut title: String = session.title.chars().collect();
            if let Some(subtitle) = &session.subtitle {
                title.push_str(" \u{00B7} ");
                title.push_str(subtitle);
            }
            // Dłuższy tytuł kończy się wielokropkiem, żeby ucięcie było
            // widoczne jako ucięcie, a nie jako koniec zdania.
            let title: String = if title.chars().count() > room {
                let mut cut: String = title.chars().take(room.saturating_sub(1)).collect();
                cut.push('\u{2026}');
                cut
            } else {
                title
            };
            row.put(CONTENT_LEFT + 1, &title, base);
            rows.push(row);
        }
    }

    // Puste wiersze do wysokości listy — panel jest stały, nie „przycina się".
    while rows.len() < (1 + panel_h).saturating_sub(2) as usize {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        rows.push(row);
    }

    // Stopka: segmenty rozdzielone `  │  `, ucinane od końca przy wąskim oknie.
    let mut footer = Row::blank(width, blank);
    footer.put(0, "\u{2502}", border);
    footer.put(width.saturating_sub(1), "\u{2502}", border);
    let mut text = String::new();
    for (i, segment) in RESUME_FOOTER.iter().enumerate() {
        let candidate = if i == 0 {
            segment.to_string()
        } else {
            format!("{}{}{}", text, FOOTER_SEPARATOR, segment)
        };
        if (candidate.chars().count() as u16) + 3 > width {
            break;
        }
        text = candidate;
    }
    footer.put(CONTENT_LEFT, &text, dim_style());
    rows.push(footer);

    rows.push(edge(width, '\u{2514}', '\u{2518}', border));

    if let Some(note) = &state.note {
        if (rows.len() as u16) < height {
            let mut row = Row::blank(width, blank);
            row.put(CONTENT_LEFT, note, Style::default().fg(YELLOW).bg(BG));
            rows.push(row);
        }
    }

    rows.truncate(height as usize);
    rows
}

// ── Modal „Select model" (runda 14) ─────────────────────────────────────────

/// Jeden wiersz pickera — gotowy do narysowania, bez wiedzy o `models.rs`.
pub struct PickerRow {
    pub name: String,
    pub id: String,
    /// Okno kontekstu jako tekst: `1.0M` albo `?`.
    pub context: String,
    /// Cena input/output za 1M albo `—`.
    pub price: String,
    /// Aktywny model w sesji — znacznik `●`.
    pub active: bool,
}

pub struct PickerState {
    pub path: String,
    pub models: Vec<PickerRow>,
    pub selected: usize,
    pub note: Option<String>,
}

/// Stopka modala — ten sam format `klawisz:opis  │  …` co w „Resume session".
const PICKER_FOOTER: [&str; 3] = ["\u{2191}/\u{2193}:navigate", "Enter:select", "Esc:back"];

/// Modal `/model`: pełna ramka, pasek tytułu, lista modeli, stopka klawiszy.
pub fn picker(state: &PickerState, width: u16, height: u16) -> Vec<Row> {
    let width = width.max(1);
    let blank = Style::default().bg(BG);
    let border = border_style();
    let mut rows: Vec<Row> = Vec::new();

    let mut header = Row::blank(width, blank);
    header.put(CONTENT_LEFT, &state.path, dim_style());
    rows.push(header);

    let panel_h = height.saturating_sub(1).max(3);
    // Ramki 2 + tytuł + separator + stopka = 5; reszta to lista.
    let list_h = panel_h.saturating_sub(5);

    rows.push(edge(width, '\u{250C}', '\u{2510}', border));

    let mut title = Row::blank(width, blank);
    title.put(0, "\u{2502}", border);
    title.put(width.saturating_sub(1), "\u{2502}", border);
    title.put(CONTENT_LEFT + 1, "Select model", text_style());
    title.right(width.saturating_sub(1), "[\u{2717}]", dim_style());
    rows.push(title);

    rows.push(edge(width, '\u{251C}', '\u{2524}', border));

    let visible = list_h.max(1) as usize;
    let first = state.selected.saturating_sub(visible.saturating_sub(1));
    for (i, model) in state.models.iter().enumerate().skip(first).take(visible) {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        if i == state.selected {
            row.fill(1, width.saturating_sub(1), Style::default().bg(crate::tui::BG_LIGHT));
        }
        let base = if i == state.selected {
            Style::default().fg(FG).bg(crate::tui::BG_LIGHT)
        } else {
            text_style()
        };
        let dim = if i == state.selected {
            Style::default().fg(GRAY).bg(crate::tui::BG_LIGHT)
        } else {
            dim_style()
        };
        // Znacznik aktywnego modelu: `●` przy wybranym w sesji, `○` przy reszcie.
        let mark = if model.active { "\u{25CF}" } else { "\u{25CB}" };
        row.put(
            CONTENT_LEFT,
            mark,
            if model.active {
                Style::default().fg(GREEN).bg(if i == state.selected {
                    crate::tui::BG_LIGHT
                } else {
                    BG
                })
            } else {
                dim
            },
        );
        // Kolumny: nazwa (do 24 znaków), id, okno kontekstu, cena.
        let name_room = 24usize.min(
            (width as usize)
                .saturating_sub(CONTENT_LEFT as usize + 3 + 26 + 8 + 17)
                .max(6),
        );
        let name = truncate_chars(&model.name, name_room);
        row.put(CONTENT_LEFT + 2, &name, base);
        if width >= 72 {
            let id_x = CONTENT_LEFT + 2 + name_room as u16 + 1;
            row.put(id_x, &truncate_chars(&model.id, 24), dim);
        }
        let ctx = model.context.clone();
        row.right(width.saturating_sub(22).max(3), &ctx, dim);
        if width >= 72 {
            row.right(width.saturating_sub(3), &model.price, dim);
        }
        rows.push(row);
    }
    if state.models.is_empty() {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        row.put(CONTENT_LEFT + 1, "Brak listy modeli", dim_style());
        rows.push(row);
    }

    // Puste wiersze do wysokości panelu — ramka nie „przycina się".
    while rows.len() < (1 + panel_h).saturating_sub(2) as usize {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        rows.push(row);
    }

    let mut footer = Row::blank(width, blank);
    footer.put(0, "\u{2502}", border);
    footer.put(width.saturating_sub(1), "\u{2502}", border);
    let mut text = String::new();
    for (i, segment) in PICKER_FOOTER.iter().enumerate() {
        let candidate = if i == 0 {
            segment.to_string()
        } else {
            format!("{}{}{}", text, FOOTER_SEPARATOR, segment)
        };
        if (candidate.chars().count() as u16) + 3 > width {
            break;
        }
        text = candidate;
    }
    footer.put(CONTENT_LEFT, &text, dim_style());
    rows.push(footer);

    rows.push(edge(width, '\u{2514}', '\u{2518}', border));

    if let Some(note) = &state.note {
        if (rows.len() as u16) < height {
            let mut row = Row::blank(width, blank);
            row.put(CONTENT_LEFT, note, Style::default().fg(YELLOW).bg(BG));
            rows.push(row);
        }
    }

    rows.truncate(height as usize);
    rows
}

/// Ucinanie do `n` znaków z wielokropkiem — nazwy modeli bywają długie.
fn truncate_chars(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(n.saturating_sub(1)).collect();
    cut.push('\u{2026}');
    cut
}

/// Wiersz ramki: narożniki i kreski między nimi.
fn edge(width: u16, left: char, right: char, style: Style) -> Row {
    let mut row = Row::blank(width, Style::default().bg(BG));
    row.put(0, &left.to_string(), style);
    for x in 1..width.saturating_sub(1) {
        row.put(x, "\u{2500}", style);
    }
    row.put(width.saturating_sub(1), &right.to_string(), style);
    row
}

// ── Modal `/settings` (runda 15) ────────────────────────────────────────────

/// Wiersz modala ustawień: etykieta, wartość i to, czy wolno ją zmieniać.
pub struct SettingRow {
    pub label: String,
    pub value: String,
    /// `false` dla wierszy „z env" — środowisko ma pierwszeństwo i jest tylko
    /// pokazywane.
    pub editable: bool,
    pub from_env: bool,
}

pub struct SettingsState {
    pub path: String,
    pub rows: Vec<SettingRow>,
    pub selected: usize,
    /// Ostatnia zmiana (`motyw: jasny`) — jedna linia pod ramką.
    pub note: Option<String>,
}

const SETTINGS_FOOTER: [&str; 3] = [
    "\u{2191}/\u{2193}:navigate",
    "Enter/\u{2190}\u{2192}:change",
    "Esc:close",
];

/// Modal `/settings`: ta sama ramka i pasek tytułu co „Select model"
/// (`picker`), lista ustawień z wartościami po prawej i stopka klawiszy.
pub fn settings(state: &SettingsState, width: u16, height: u16) -> Vec<Row> {
    let width = width.max(1);
    let blank = Style::default().bg(BG);
    let border = border_style();
    let mut rows: Vec<Row> = Vec::new();

    let mut header = Row::blank(width, blank);
    header.put(CONTENT_LEFT, &state.path, dim_style());
    rows.push(header);

    // Notatka po zmianie dostaje własny wiersz pod ramką — bez rezerwacji
    // wypełnienie panelu zjadałoby całą wysokość ekranu.
    let note_rows = u16::from(state.note.is_some() && height > 8);
    let panel_h = height.saturating_sub(1 + note_rows).max(3);
    // Ramki 2 + tytuł + separator + stopka = 5; reszta to lista ustawień.
    let list_h = panel_h.saturating_sub(5);

    rows.push(edge(width, '\u{250C}', '\u{2510}', border));

    let mut title = Row::blank(width, blank);
    title.put(0, "\u{2502}", border);
    title.put(width.saturating_sub(1), "\u{2502}", border);
    title.put(CONTENT_LEFT + 1, "Settings", text_style());
    title.right(width.saturating_sub(1), "[\u{2717}]", dim_style());
    rows.push(title);

    rows.push(edge(width, '\u{251C}', '\u{2524}', border));

    let visible = list_h.max(1) as usize;
    // Zaznaczenie zawsze w kadrze, także przy dłuższej liście (env dochodzi).
    let first = if state.selected >= visible {
        state.selected + 1 - visible
    } else {
        0
    };
    for (i, setting) in state.rows.iter().enumerate().skip(first).take(visible) {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        let selected = i == state.selected;
        if selected {
            row.fill(1, width.saturating_sub(1), Style::default().bg(crate::tui::BG_LIGHT));
        }
        let base = if selected {
            Style::default().fg(FG).bg(crate::tui::BG_LIGHT)
        } else {
            text_style()
        };
        let dim = if selected {
            Style::default().fg(GRAY).bg(crate::tui::BG_LIGHT)
        } else {
            dim_style()
        };
        // Znacznik zaznaczenia tym samym znakiem co w pickerze modeli.
        row.put(
            CONTENT_LEFT,
            if selected { "\u{25CF}" } else { "\u{25CB}" },
            if selected {
                Style::default().fg(GREEN).bg(crate::tui::BG_LIGHT)
            } else {
                dim
            },
        );
        row.put(CONTENT_LEFT + 2, &truncate_chars(&setting.label, 28), base);
        let value = if setting.from_env {
            format!("z env  {}", setting.value)
        } else {
            setting.value.clone()
        };
        // Wartość po prawej; przy zaznaczeniu strzałki pokazują, że `←/→`
        // (i `Enter`) zmieniają wartość.
        let text = if selected && setting.editable {
            format!("\u{2039} {} \u{203A}", value)
        } else {
            value
        };
        let style = if setting.from_env { dim } else if selected { base } else { body_style() };
        row.right(width.saturating_sub(3), &truncate_chars(&text, 30), style);
        rows.push(row);
    }

    // Puste wiersze do wysokości panelu.
    while rows.len() < (1 + panel_h).saturating_sub(2) as usize {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        rows.push(row);
    }

    let mut footer = Row::blank(width, blank);
    footer.put(0, "\u{2502}", border);
    footer.put(width.saturating_sub(1), "\u{2502}", border);
    let mut text = String::new();
    for (i, segment) in SETTINGS_FOOTER.iter().enumerate() {
        let candidate = if i == 0 {
            segment.to_string()
        } else {
            format!("{}{}{}", text, FOOTER_SEPARATOR, segment)
        };
        if (candidate.chars().count() as u16) + 3 > width {
            break;
        }
        text = candidate;
    }
    footer.put(CONTENT_LEFT, &text, dim_style());
    rows.push(footer);

    rows.push(edge(width, '\u{2514}', '\u{2518}', border));

    if let Some(note) = &state.note {
        if (rows.len() as u16) < height {
            let mut row = Row::blank(width, blank);
            row.put(CONTENT_LEFT, note, Style::default().fg(YELLOW).bg(BG));
            rows.push(row);
        }
    }

    rows.truncate(height as usize);
    rows
}

// ── Modal `/skills` (runda 16) ──────────────────────────────────────────────

/// Wiersz listy skilli: nazwa, opis, rozmiar treści i znacznik „w kontekście".
pub struct SkillRow {
    pub name: String,
    pub description: String,
    /// `412 B` / `1.2 KB` — rozmiar treści bez nagłówka.
    pub size: String,
    /// Zepsuty nagłówek YAML: skill jest na liście, ale nie da się go wczytać.
    pub broken: bool,
    /// Treść już w kontekście (`/skill` albo `skill.load` modelu).
    pub loaded: bool,
}

pub struct SkillsState {
    pub path: String,
    pub rows: Vec<SkillRow>,
    pub selected: usize,
    pub note: Option<String>,
}

/// Szerokość kolumny znacznika (`✓ w kontekście` / `! błąd nagłówka`).
const MARKER_COL: u16 = 16;

const SKILLS_FOOTER: [&str; 3] = [
    "\u{2191}/\u{2193}:navigate",
    "Enter:preview",
    "Esc:close",
];

/// Modal `/skills`: ta sama ramka co „Select model" i `/settings`, wiersze
/// `nazwa · opis · rozmiar` i znacznik `w kontekście`. Enter podgląda treść
/// w scrollbacku, Esc zamyka (modal nie zmienia stanu sam z siebie).
pub fn skills(state: &SkillsState, width: u16, height: u16) -> Vec<Row> {
    let width = width.max(1);
    let blank = Style::default().bg(BG);
    let border = border_style();
    let mut rows: Vec<Row> = Vec::new();

    let mut header = Row::blank(width, blank);
    header.put(CONTENT_LEFT, &state.path, dim_style());
    rows.push(header);

    let note_rows = u16::from(state.note.is_some() && height > 8);
    let panel_h = height.saturating_sub(1 + note_rows).max(3);
    // Ramki 2 + tytuł + separator + stopka = 5 wierszy poza listą.
    let list_h = panel_h.saturating_sub(5);

    rows.push(edge(width, '\u{250C}', '\u{2510}', border));

    let mut title = Row::blank(width, blank);
    title.put(0, "\u{2502}", border);
    title.put(width.saturating_sub(1), "\u{2502}", border);
    title.put(CONTENT_LEFT + 1, "Skills", text_style());
    let count = format!("{} skilli", state.rows.len());
    title.right(width.saturating_sub(6), &count, dim_style());
    title.right(width.saturating_sub(1), "[\u{2717}]", dim_style());
    rows.push(title);

    rows.push(edge(width, '\u{251C}', '\u{2524}', border));

    let visible = list_h.max(1) as usize;
    let first = if state.selected >= visible {
        state.selected + 1 - visible
    } else {
        0
    };
    if state.rows.is_empty() {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        row.put(
            CONTENT_LEFT + 2,
            "Brak skilli \u{2014} dodaj skills/<nazwa>/SKILL.md",
            dim_style(),
        );
        rows.push(row);
    }
    for (i, skill) in state.rows.iter().enumerate().skip(first).take(visible) {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        let selected = i == state.selected;
        if selected {
            row.fill(1, width.saturating_sub(1), Style::default().bg(crate::tui::BG_LIGHT));
        }
        let base = if selected {
            Style::default().fg(FG).bg(crate::tui::BG_LIGHT)
        } else {
            text_style()
        };
        let dim = if selected {
            Style::default().fg(GRAY).bg(crate::tui::BG_LIGHT)
        } else {
            dim_style()
        };
        row.put(
            CONTENT_LEFT,
            if selected { "\u{25CF}" } else { "\u{25CB}" },
            if selected {
                Style::default().fg(GREEN).bg(crate::tui::BG_LIGHT)
            } else {
                dim
            },
        );
        // Nazwa w stałej kolumnie, żeby opisy zaczynały się w jednej linii.
        row.put(CONTENT_LEFT + 2, &truncate_chars(&skill.name, 20), base);
        let describe = CONTENT_LEFT + 24;
        // Prawa strona: rozmiar w stałej kolumnie i znacznik
        // (`✓ w kontekście` / `! błąd nagłówka`) — znacznik zawsze na końcu,
        // więc rozmiary stoją w jednej linii.
        let marker = if skill.loaded {
            "\u{2713} w kontek\u{15B}cie"
        } else if skill.broken {
            "! bl\u{105}d nag\u{142}\u{00F3}wka"
        } else {
            ""
        };
        let marker_w = marker.chars().count() as u16;
        // Kolumna rozmiaru jest wspólna dla wszystkich wierszy (rezerwacja na
        // najdłuższy znacznik), inaczej liczby skakałyby w prawo i lewo.
        let gap = MARKER_COL + 2;
        let size_right = width.saturating_sub(3).saturating_sub(gap);
        let right_w = skill.size.chars().count() as u16 + gap;
        let room = width
            .saturating_sub(2)
            .saturating_sub(describe)
            .saturating_sub(right_w + 2) as usize;
        let desc = truncate_chars(&skill.description, room.max(4));
        let desc_style = if selected {
            base
        } else if skill.broken {
            dim
        } else {
            body_style()
        };
        row.put(describe, &desc, desc_style);
        let size_style = if selected {
            base
        } else {
            dim
        };
        row.right(size_right, &skill.size, size_style);
        if marker_w > 0 {
            row.right(
                width.saturating_sub(3),
                marker,
                if selected {
                    base
                } else if skill.loaded {
                    Style::default().fg(GREEN).bg(BG)
                } else {
                    dim
                },
            );
        }
        rows.push(row);
    }

    while rows.len() < (1 + panel_h).saturating_sub(2) as usize {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        rows.push(row);
    }

    let mut footer = Row::blank(width, blank);
    footer.put(0, "\u{2502}", border);
    footer.put(width.saturating_sub(1), "\u{2502}", border);
    let mut text = String::new();
    for (i, segment) in SKILLS_FOOTER.iter().enumerate() {
        let candidate = if i == 0 {
            segment.to_string()
        } else {
            format!("{}{}{}", text, FOOTER_SEPARATOR, segment)
        };
        if (candidate.chars().count() as u16) + 3 > width {
            break;
        }
        text = candidate;
    }
    footer.put(CONTENT_LEFT, &text, dim_style());
    rows.push(footer);

    rows.push(edge(width, '\u{2514}', '\u{2518}', border));

    if let Some(note) = &state.note {
        if (rows.len() as u16) < height {
            let mut row = Row::blank(width, blank);
            row.put(CONTENT_LEFT, note, Style::default().fg(YELLOW).bg(BG));
            rows.push(row);
        }
    }

    rows.truncate(height as usize);
    rows
}

// ── Modal wyszukiwania w scrollbacku (runda 17) ────────────────────────────

/// Jedno trafienie: numer wiersza scrollbacku (1-based) i jego tekst.
pub struct SearchHit {
    pub line: usize,
    pub text: String,
}

pub struct SearchState {
    pub path: String,
    pub query: String,
    pub hits: Vec<SearchHit>,
    /// Indeks zaznaczonego trafienia (0 = pierwsze).
    pub selected: usize,
    /// `true` gdy znaki idą do zapytania; po `Enter` modal przechodzi w tryb
    /// chodzenia po trafieniach (`n`/`N`).
    pub editing: bool,
    pub note: Option<String>,
}

const SEARCH_FOOTER: [&str; 4] = [
    "Esc:back",
    "Enter:jump",
    "n/N:next/prev",
    "ctrl+f:close",
];

/// Modal `Ctrl+F` w stylu „Select model": pasek `/ szukaj`, licznik `1/3`,
/// lista trafień (numer wiersza + tekst) i stopka klawiszy.
pub fn search(state: &SearchState, width: u16, height: u16) -> Vec<Row> {
    let width = width.max(1);
    let blank = Style::default().bg(BG);
    let border = border_style();
    let mut rows: Vec<Row> = Vec::new();

    let mut header = Row::blank(width, blank);
    header.put(CONTENT_LEFT, &state.path, dim_style());
    rows.push(header);

    let panel_h = height.saturating_sub(1).max(3);
    // Ramki 2 + tytuł + separator + pasek + kreska + stopka = 7.
    let reserved = 7u16;
    let list_h = panel_h.saturating_sub(reserved);

    rows.push(edge(width, '\u{250C}', '\u{2510}', border));

    let mut title = Row::blank(width, blank);
    title.put(0, "\u{2502}", border);
    title.put(width.saturating_sub(1), "\u{2502}", border);
    title.put(CONTENT_LEFT + 1, "Search scrollback", text_style());
    title.right(width.saturating_sub(1), "[\u{2717}]", dim_style());
    rows.push(title);

    rows.push(edge(width, '\u{251C}', '\u{2524}', border));

    // Pasek szukania: `/ szukaj` (pusto) albo `/ fraza`; licznik po prawej.
    let mut tools = Row::blank(width, blank);
    tools.put(0, "\u{2502}", border);
    tools.put(width.saturating_sub(1), "\u{2502}", border);
    let left = if state.query.is_empty() {
        "/ szukaj".to_string()
    } else {
        format!("/ {}", state.query)
    };
    tools.put(CONTENT_LEFT, &left, dim_style());
    if !state.query.is_empty() {
        let counter = format!(
            "{}/{}",
            if state.hits.is_empty() { 0 } else { state.selected + 1 },
            state.hits.len()
        );
        tools.right(width.saturating_sub(3), &counter, dim_style());
    }
    rows.push(tools);

    // Kreska pod paskiem — jak w „Resume session".
    let mut rule = Row::blank(width, blank);
    rule.put(0, "\u{2502}", border);
    rule.put(width.saturating_sub(1), "\u{2502}", border);
    for x in 1..width.saturating_sub(1) {
        rule.put(x, "\u{2500}", border);
    }
    rows.push(rule);

    if state.hits.is_empty() {
        if !state.query.is_empty() {
            let mut row = Row::blank(width, blank);
            row.put(0, "\u{2502}", border);
            row.put(width.saturating_sub(1), "\u{2502}", border);
            row.put(CONTENT_LEFT + 1, "brak trafien", dim_style());
            rows.push(row);
        }
    } else {
        let visible = list_h.max(1) as usize;
        // Okno listy trzyma zaznaczenie w kadrze, tak jak picker modeli.
        let first = state.selected.saturating_sub(visible.saturating_sub(1));
        for (i, hit) in state.hits.iter().enumerate().skip(first).take(visible) {
            let mut row = Row::blank(width, blank);
            row.put(0, "\u{2502}", border);
            row.put(width.saturating_sub(1), "\u{2502}", border);
            if i == state.selected {
                row.fill(1, width.saturating_sub(1), Style::default().bg(crate::tui::BG_LIGHT));
            }
            let base = if i == state.selected {
                Style::default().fg(FG).bg(crate::tui::BG_LIGHT)
            } else {
                text_style()
            };
            let dim = if i == state.selected {
                Style::default().fg(GRAY).bg(crate::tui::BG_LIGHT)
            } else {
                dim_style()
            };
            let number = format!("{:>3}", hit.line);
            row.put(CONTENT_LEFT, &number, dim);
            let room = (width as usize)
                .saturating_sub(CONTENT_LEFT as usize + 6)
                .max(4);
            row.put(CONTENT_LEFT + 5, &truncate_chars(&hit.text, room), base);
            rows.push(row);
        }
    }

    // Puste wiersze do wysokości listy — panel nie „przycina się".
    while rows.len() < (1 + panel_h).saturating_sub(2) as usize {
        let mut row = Row::blank(width, blank);
        row.put(0, "\u{2502}", border);
        row.put(width.saturating_sub(1), "\u{2502}", border);
        rows.push(row);
    }

    let mut footer = Row::blank(width, blank);
    footer.put(0, "\u{2502}", border);
    footer.put(width.saturating_sub(1), "\u{2502}", border);
    let mut text = String::new();
    for (i, segment) in SEARCH_FOOTER.iter().enumerate() {
        let candidate = if i == 0 {
            segment.to_string()
        } else {
            format!("{}{}{}", text, FOOTER_SEPARATOR, segment)
        };
        if (candidate.chars().count() as u16) + 3 > width {
            break;
        }
        text = candidate;
    }
    // Tryb pisania podpowiada, że `n` to jeszcze litera frazy.
    if state.editing && (text.chars().count() as u16) + 20 < width {
        text.push_str("  \u{00B7}  wpisz fraze");
    }
    footer.put(CONTENT_LEFT, &text, dim_style());
    rows.push(footer);

    rows.push(edge(width, '\u{2514}', '\u{2518}', border));

    if let Some(note) = &state.note {
        if (rows.len() as u16) < height {
            let mut row = Row::blank(width, blank);
            row.put(CONTENT_LEFT, note, Style::default().fg(YELLOW).bg(BG));
            rows.push(row);
        }
    }

    rows.truncate(height as usize);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::{Badge, State};

    fn welcome_state() -> WelcomeState {
        WelcomeState {
            path: "~/projects/ArndtAi".to_string(),
            model: "mimo-v2.6-flash".to_string(),
            mode: "always-approve".to_string(),
            version: "0.1.0".to_string(),
            announcements: vec![
                "ArndtAi TUI jest gotowe.".to_string(),
                "Wybierz 'Resume session' albo wpisz zadanie.".to_string(),
            ],
            tip: "Tip: Ctrl+R wznawia sesje, Ctrl+W otwiera liste.".to_string(),
            note: None,
        }
    }

    fn row_of() -> SessionRow {
        SessionRow {
            id: "700".to_string(),
            title: "write alfa.txt".to_string(),
            subtitle: Some("main".to_string()),
            state: State::Done,
            activity: None,
            badges: Vec::<Badge>::new(),
            cwd: "~/projects/ArndtAi".to_string(),
            last_change: 0,
            steps: 1,
            tool_calls: 1,
            denied: 0,
        }
    }

    fn resume_state<'a>(rows: &'a [SessionRow]) -> ResumeState<'a> {
        ResumeState {
            path: "~/projects/ArndtAi".to_string(),
            sessions: rows,
            selected: 0,
            offset: 0,
            query: String::new(),
            searching: false,
            filter: Filter::All,
            now: 120,
            note: None,
        }
    }

    fn cols(row: &Row, from: usize, to: usize) -> String {
        row.text().chars().skip(from).take(to - from).collect()
    }

    /// 83 kolumny to szerokość zrzutu: każdy element musi wypaść w tej samej
    /// kolumnie co w `welcome.txt`.
    #[test]
    fn welcome_matches_the_reference_at_83_columns() {
        let rows = welcome(&welcome_state(), 83, 38);
        let text: Vec<String> = rows.iter().map(|r| r.text()).collect();

        assert_eq!(rows.len(), 19, "19 wierszy jak w zrzucie");
        for line in &text {
            assert_eq!(line.chars().count(), 83, "wiersz 83 kolumn: {line:?}");
        }
        assert_eq!(text[0], format!("  ~/projects/ArndtAi{}", " ".repeat(63)));
        assert_eq!(text[1].trim_end(), format!("{}{}", " ".repeat(35), LOGO[0]).trim_end());
        assert_eq!(text[7].trim_end(), format!("{}{}", " ".repeat(35), LOGO[6]).trim_end());

        // Menu: etykieta w 17, skrót kończy się w 68.
        assert_eq!(cols(&rows[8], 17, 31), "Resume session");
        assert_eq!(cols(&rows[8], 62, 68), "ctrl+r");
        assert_eq!(cols(&rows[9], 17, 29), "New worktree");
        assert_eq!(cols(&rows[9], 62, 68), "ctrl+w");
        assert_eq!(text[10].trim_end(), format!("{}Changelog", " ".repeat(17)));
        assert_eq!(cols(&rows[11], 62, 68), "ctrl+q");

        // Ogłoszenia w tym samym wcięciu, `Tip:` o kolumnę dalej.
        assert_eq!(cols(&rows[12], 0, 17), " ".repeat(17));
        assert!(text[12][17..].starts_with("ArndtAi TUI"));
        assert!(text[14].starts_with("   Tip:"));

        // Composer: zaokrąglona ramka od kolumny 2 do 82.
        assert_eq!(
            text[15].trim_end(),
            format!("  \u{256D}{}\u{256E}", "\u{2500}".repeat(79))
        );
        assert_eq!(cols(&rows[16], 2, 5), "\u{2502} \u{276F}");
        assert_eq!(cols(&rows[17], 2, 3), "\u{2570}");
        assert!(text[17].contains("mimo-v2.6-flash \u{00B7} always-approve"));
        assert_eq!(cols(&rows[17], 81, 83), "\u{2500}\u{256F}");

        // Wersja: przy prawej krawędzi, dwie kolumny pod ramką.
        assert_eq!(
            text[18].trim_end(),
            format!("{}ArndtAi 0.1.0", " ".repeat(70))
        );
    }

    /// Logo to znak z `welcome.txt`: 7 wierszy × 14 kolumn braille, lewa
    /// krawędź w kolumnie 35. Kształt jest przepisany, nie rysowany od nowa —
    /// większy niż 14 kolumn nie zmieściłby się w zrzucie referencyjnym.
    #[test]
    fn logo_is_the_reference_shape() {
        const REF: [&str; 7] = [
            "⠀⠀⠀⠀⠀⠀⣀⣀⡀⠀⠀⠀⢀⠄",
            "⠀⠀⠀⣠⣾⠿⠛⠛⠛⠛⢀⡴⠁⠀",
            "⠀⠀⣼⡟⠁⠀⠀⠀⢀⡴⠻⣿⡀⠀",
            "⠀⠀⣿⡇⠀⠀⠀⠔⠁⠀⠀⣿⡇⠀",
            "⠀⠀⢹⣷⠀⠀⠀⠀⠀⢀⣴⡿⠀⠀",
            "⠀⢀⠞⠁⠠⢶⣶⣶⣶⠿⠋⠀⠀⠀",
            "⠐⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀",
        ];
        assert_eq!(LOGO, REF);
        assert_eq!(LOGO_WIDTH, 14);
    }

    /// Szerszy terminal: logo i menu nadal wyśrodkowane, composer szerszy,
    /// żadna linia nie wychodzi za ekran.
    #[test]
    fn welcome_stays_centered_at_120_columns() {
        let rows = welcome(&welcome_state(), 120, 38);
        let text: Vec<String> = rows.iter().map(|r| r.text()).collect();
        assert_eq!(rows.len(), 19);
        for line in &text {
            assert_eq!(line.chars().count(), 120, "wiersz musi mieć 120 kolumn");
        }
        assert_eq!(text[1].trim_end(), format!("{}{}", " ".repeat(54), LOGO[0]).trim_end());
        assert_eq!(cols(&rows[8], 35, 49), "Resume session");
        assert_eq!(cols(&rows[8], 80, 86), "ctrl+r");
        assert_eq!(
            text[15].trim_end(),
            format!("  \u{256D}{}\u{256E}", "\u{2500}".repeat(116))
        );
        assert!(text[18].trim_end().ends_with("ArndtAi 0.1.0"));
    }

    /// 40 kolumn: wszystko musi się zmieścić — skróty zostają, etykieta
    /// composera przycina się do miejsca w dolnej krawędzi.
    #[test]
    fn welcome_fits_40_columns() {
        let rows = welcome(&welcome_state(), 40, 20);
        let text: Vec<String> = rows.iter().map(|r| r.text()).collect();
        assert_eq!(rows.len(), 19, "niski ekran tnie końcówkę, nie cały układ");
        for line in &text {
            assert_eq!(line.chars().count(), 40, "wiersz 40 kolumn: {line:?}");
        }
        assert_eq!(text[1].trim_end(), format!("{}{}", " ".repeat(14), LOGO[0]).trim_end());
        assert_eq!(cols(&rows[8], 2, 16), "Resume session");
        assert_eq!(cols(&rows[8], 34, 40), "ctrl+r");
        assert_eq!(
            text[15].trim_end(),
            format!("  \u{256D}{}\u{256E}", "\u{2500}".repeat(36))
        );
        assert!(text[17].ends_with("\u{2500}\u{256F}"));
        assert!(text[18].trim_end().ends_with("ArndtAi 0.1.0"));
    }

    /// 85 kolumn to szerokość zrzutu modala: ramki, tytuł z `[✗]`, pasek
    /// narzędzi i stopka muszą wypaść w tych samych kolumnach.
    #[test]
    fn resume_matches_the_reference_at_85_columns() {
        let rows = resume(&resume_state(&[]), 85, 53);
        let text: Vec<String> = rows.iter().map(|r| r.text()).collect();
        let footer = "Esc:back  \u{2502}  Enter:select  \u{2502}  ctrl+w:worktree  \u{2502}  \u{2191}/\u{2193}:navigate  \u{2502}  f:filter";

        assert_eq!(rows.len(), 53);
        assert_eq!(text[0], format!("  ~/projects/ArndtAi{}", " ".repeat(65)));
        assert_eq!(text[1], format!("\u{250C}{}\u{2510}", "\u{2500}".repeat(83)));
        assert_eq!(cols(&rows[2], 3, 17), "Resume session");
        assert_eq!(cols(&rows[2], 81, 84), "[\u{2717}]");
        assert_eq!(cols(&rows[2], 84, 85), "\u{2502}");
        assert_eq!(text[3], format!("\u{251C}{}\u{2524}", "\u{2500}".repeat(83)));
        assert_eq!(cols(&rows[4], 2, 13), "/ to search");
        assert_eq!(cols(&rows[4], 72, 82), "0 sessions");
        assert_eq!(cols(&rows[4], 84, 85), "\u{2502}");
        assert_eq!(text[5], format!("\u{2502}{}\u{2502}", "\u{2500}".repeat(83)));
        assert_eq!(cols(&rows[6], 3, 13), "No matches");
        // Stopka jest przedostatnim wierszem panelu, pod nią dolna krawędź.
        assert_eq!(cols(&rows[51], 2, 77), footer);
        assert_eq!(cols(&rows[51], 77, 84), " ".repeat(7));
        assert_eq!(text[51].chars().last(), Some('\u{2502}'));
        assert_eq!(text[51].chars().count(), 85);
        assert_eq!(text[52], format!("\u{2514}{}\u{2518}", "\u{2500}".repeat(83)));
    }

    /// Ten sam panel przy 83 i 120 kolumnach: ramka domyka się na krawędzi,
    /// `[✗]` siedzi przy prawej ramce, stopka nie wychodzi za ekran.
    #[test]
    fn resume_holds_the_panel_at_83_and_120_columns() {
        for width in [83u16, 120] {
            let rows = resume(&resume_state(&[]), width, 38);
            assert_eq!(rows.len(), 38);
            for row in &rows {
                assert_eq!(row.text().chars().count(), width as usize);
            }
            assert_eq!(
                rows[1].text(),
                format!("\u{250C}{}\u{2510}", "\u{2500}".repeat(width as usize - 2))
            );
            assert_eq!(cols(&rows[2], 3, 17), "Resume session");
            assert_eq!(cols(&rows[2], width as usize - 4, width as usize - 1), "[\u{2717}]");
            assert!(rows[37].text().starts_with('\u{2514}'));
            assert!(rows[36].text().contains("f:filter"), "stopka się mieści");
        }
    }

    /// Wpisy listy: tytuł, przygaszony ` · subtitle` i wiek przy prawej;
    /// zaznaczenie to pasek tła (`bg_light`), nie sam kolor tekstu.
    #[test]
    fn resume_rows_carry_title_subtitle_age_and_selection() {
        let sessions = [row_of()];
        let mut state = resume_state(&sessions);
        state.now = 3_700;
        let rows = resume(&state, 85, 20);
        let text = rows[6].text();
        let after_border: String = text.chars().skip(3).collect();
        assert!(
            after_border.starts_with("write alfa.txt \u{00B7} main"),
            "{text:?}"
        );
        assert!(text.trim_end().ends_with("1h  \u{2502}"), "wiek przy prawej: {text:?}");
        assert_eq!(rows[6].cells[1].style.bg, Some(crate::tui::BG_LIGHT));
    }

    #[test]
    fn resume_narrows_to_40_columns_without_overflow() {
        let sessions = [row_of()];
        for height in [38u16, 20, 8] {
            let rows = resume(&resume_state(&sessions), 40, height);
            assert_eq!(rows.len(), height as usize);
            let text: Vec<String> = rows.iter().map(|r| r.text()).collect();
            for line in &text {
                assert_eq!(line.chars().count(), 40, "{height}: {line:?}");
            }
            assert_eq!(cols(&rows[2], 3, 17), "Resume session");
            assert_eq!(cols(&rows[2], 36, 39), "[\u{2717}]");
            assert!(text[height as usize - 1].starts_with('\u{2514}'));
            assert!(text[4].contains("/ to search"));
        }
    }

    #[test]
    fn resume_says_no_matches_when_the_query_filters_everything() {
        let mut state = resume_state(&[]);
        state.query = "zzz".to_string();
        state.searching = true;
        let rows = resume(&state, 85, 20);
        let text = rows[4].text();
        assert!(text.contains("/ zzz"), "wpisany tekst widoczny w pasku: {text:?}");
        assert!(text.contains("0 sessions"));
        assert!(rows[6].text().contains("No matches"));
    }

    // ── Runda 15: modal `/settings` ────────────────────────────────────────

    fn settings_state() -> SettingsState {
        SettingsState {
            path: "~/projects/ArndtAi".to_string(),
            rows: vec![
                SettingRow {
                    label: "motyw".to_string(),
                    value: "jasny".to_string(),
                    editable: true,
                    from_env: false,
                },
                SettingRow {
                    label: "tryb zgod".to_string(),
                    value: "ask".to_string(),
                    editable: true,
                    from_env: false,
                },
                SettingRow {
                    label: "ARNDTAI_MODEL".to_string(),
                    value: "mimo-v2.6-flash".to_string(),
                    editable: false,
                    from_env: true,
                },
            ],
            selected: 0,
            note: Some("motyw: jasny".to_string()),
        }
    }

    #[test]
    fn settings_modal_has_the_picker_frame_and_a_title() {
        for height in [20u16, 38] {
            let rows = settings(&settings_state(), 83, height);
            assert_eq!(rows.len(), height as usize, "modal wypelnia kadr");
            for row in &rows {
                assert_eq!(row.text().chars().count(), 83, "{:?}", row.text());
            }
            let text: Vec<String> = rows.iter().map(|r| r.text()).collect();
            assert_eq!(cols(&rows[2], 3, 11), "Settings", "pasek tytulu");
            assert!(text[2].contains("\u{2717}"), "znacznik zamkniecia");
            assert!(text[0].contains("~/projects/ArndtAi"), "sciezka w naglowku");
            assert!(text.iter().any(|line| line.contains("Esc:close")), "stopka");
            assert!(
                text.iter().any(|line| line.starts_with('\u{2514}')),
                "dolna ramka"
            );
        }
    }

    #[test]
    fn settings_modal_marks_the_selected_row_and_env_rows() {
        let rows = settings(&settings_state(), 83, 24);
        let text: Vec<String> = rows.iter().map(|r| r.text()).collect();
        // Zaznaczony wiersz: `●` przed etykietą, wartość w nawiasach strzałek.
        let selected = text
            .iter()
            .find(|line| line.contains("motyw"))
            .expect("wiersz motywu");
        assert!(selected.contains('\u{25CF}'), "znacznik zaznaczenia: {selected}");
        assert!(
            selected.contains("\u{2039} jasny \u{203A}"),
            "strzalki przy edytowalnej wartosci: {selected}"
        );
        // Wiersz ze środowiska: `z env` i bez strzałek (nieedytowalny).
        let env = text
            .iter()
            .find(|line| line.contains("ARNDTAI_MODEL"))
            .expect("wiersz env");
        assert!(env.contains("z env"), "oznaczenie z env: {env}");
        assert!(!env.contains('\u{2039}'), "env nie jest edytowalny: {env}");
        // Notatka pod ramką.
        assert!(
            text.last().unwrap().contains("motyw: jasny"),
            "notatka po zmianie: {:?}",
            text.last()
        );
    }

}

