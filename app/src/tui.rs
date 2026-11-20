//! Tryb TUI: kopia układu pagera Grok Build (`xai-grok-pager`) na eventach ArndtAi.
//!
//! Układ, metryki i paleta są wzięte z oryginału (`views/agent.rs`,
//! `views/status_bar.rs`, `views/status_line`, `scrollback/layout.rs`,
//! `theme/groknight.rs`) — mapa plik → element jest w
//! `app/docs/re/grok-build-tui.md`.
//!
//! TUI nie ma własnej pętli ani własnych bramek: woła `agent::run` i rysuje
//! dokładnie te eventy, które `context::Session::event` zapisuje do JSONL.
//! Kontrakt `docs/contracts/events.md` zostaje bez zmian — TUI jest tylko
//! dodatkowym konsumentem tego samego strumienia.

use crate::agent::{self, Config};
use crate::commands::{self, Handler};
use crate::context::{self, MemoryLevel};
use crate::models::{self, Catalog};
use crate::provider::{self, Brain};
use crate::sessions::{self, Badge, Filter, SessionRow, State as RowState};
use crate::settings::{self, Approval, Settings, Theme};
use crate::tools;
pub(crate) use crate::welcome;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::Frame;
use serde_json::Value;
use std::cell::Cell;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::markdown;

// ── Paleta: groknight (skala szarości #0a0a0a..#242424 + akcenty TokyoNight) ─
// Wartości 1:1 z `xai-grok-pager-render/src/theme/groknight.rs`.
pub(crate) const BG: Color = Color::Rgb(20, 20, 20); // bg_storm
pub(crate) const BG_LIGHT: Color = Color::Rgb(36, 36, 36); // bg_highlight
pub(crate) const BG_TERMINAL: Color = Color::Rgb(10, 10, 10); // bg_terminal
pub(crate) const FG: Color = Color::Rgb(225, 225, 225);
pub(crate) const FG_DARK: Color = Color::Rgb(200, 200, 200);
pub(crate) const GRAY: Color = Color::Rgb(108, 108, 108);
pub(crate) const GRAY_DIM: Color = Color::Rgb(88, 88, 88);
pub(crate) const GRAY_BRIGHT: Color = Color::Rgb(120, 120, 120);
#[allow(dead_code)] // paleta groknight: kolor `path`/akcentów, nieużywany po rundzie 7
pub(crate) const CYAN: Color = Color::Rgb(125, 207, 255);
pub(crate) const GREEN: Color = Color::Rgb(158, 206, 106);
pub(crate) const MAGENTA: Color = Color::Rgb(187, 154, 247);
pub(crate) const ORANGE: Color = Color::Rgb(255, 158, 100);
#[allow(dead_code)] // paleta groknight: fala na kolumnie akcentu (kolumny już nie ma)
pub(crate) const PURPLE: Color = Color::Rgb(157, 124, 216); // #9d7cd8
pub(crate) const RED: Color = Color::Rgb(247, 118, 142);
pub(crate) const TEAL: Color = Color::Rgb(26, 188, 156);
pub(crate) const YELLOW: Color = Color::Rgb(224, 175, 104);
pub(crate) const SELECTION_BORDER: Color = Color::Rgb(60, 60, 65); // selection_border
// Ze zrzutu `readme-tui.png`: ramka composera to `#323237` (50,50,55) także
// wtedy, gdy composer jest gotowy na wpis — jaśniejszy wariant zostaje tylko
// dla ramek paneli na liście sesji.
pub(crate) const PROMPT_BORDER: Color = Color::Rgb(50, 50, 55);
pub(crate) const PROMPT_BORDER_ACTIVE: Color = Color::Rgb(80, 80, 88);
// `groknight`: pas tła wstawionej linii diffu (kwantyzuje się do zieleni 256).
pub(crate) const GREEN_DARK: Color = Color::Rgb(6, 56, 6);
// `scrollback/blocks/user.rs`: `prompt_band_color_for()` → `theme.bg_light`,
// `prompt_styles()` → prefiks `accent_user`, tekst `text_primary`.
pub(crate) const ACCENT_USER: Color = Color::Rgb(200, 200, 200); // accent_user == #c8c8c8

// ── Znaki: `xai-grok-pager-render/src/glyphs.rs` (warianty bez ConHost) ──────
const PROMPT_ARROW: &str = "\u{276F} "; // ❯ + spacja, zawsze 2 kolumny
#[allow(dead_code)] // `accent_bar` pagera: u nas treść idzie pełną szerokością
const ACCENT: char = '\u{2503}'; // ┃
const BULLET: char = '\u{25C6}'; // ◆ punkt nagłówka bloku (`diamond_filled`)
const DIAMOND_HOLLOW: char = '\u{25C7}'; // ◇
const BALLOT_X: char = '\u{2717}'; // ✗
const CHECK_MARK: char = '\u{2713}'; // ✓
const SELECTION_BAR: char = '\u{258F}'; // ▏ pasek zaznaczenia wiersza
const DISCLOSURE_OPEN: char = '\u{25BE}'; // ▾ nagłówek grupy
const SEPARATOR: &str = " \u{2502} "; //  │  w pasku statusu
const HINT_SEPARATOR: &str = " \u{2502} "; //  │  w stopce (zrzut `readme-tui.png`)

/// `usage` nie ma? Tokeny liczymy z znaków transkryptu (≈4 znaki na token)
/// i oznaczamy `~`. Gdy provider przysłał `usage`, licznik pokazuje realne
/// tokeny bez tyldy — patrz `app/docs/re/tui-model-and-commands.md`.
const CHARS_PER_TOKEN: u64 = 4;
/// Poniżej tego opóźnienia nie piszemy „for 0.0s" — sama etykieta `Thought`.
const THOUGHT_MIN: Duration = Duration::from_millis(100);

/// Braille spinner — te same 8 klatek i ten sam dzielnik (4 ticki) co oryginał.
const SPINNER: [&str; 8] = [
    "\u{280B}", "\u{2819}", "\u{2839}", "\u{2838}", "\u{283C}", "\u{2834}", "\u{2826}", "\u{2827}",
];
const SPINNER_DIVISOR: u64 = 4;

/// Metryki układu z `LayoutConfig::default()` (pager-render/src/appearance).
const OUTER_VPAD: u16 = 1;
const OUTER_HPAD: u16 = 2;
const COMPACT_HPAD: u16 = 1;
const SCROLLBACK_MIN_ROWS: u16 = 5;
const SHORT_TERMINAL_ROWS: u16 = 16;
const AUTO_COMPACT_MAX_ROWS: u16 = 20;
const PROMPT_HEIGHT: u16 = 3;
const SCROLLBAR_WIDTH: u16 = 1;

const DEMO_TASK: &str =
    "write notatka.txt :: ArndtAi TUI dziala; read notatka.txt; write ../poza.txt :: x";

// ── Model wpisu ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Task,
    Thought,
    Call,
    Ok,
    Fail,
    Cached,
    Skipped,
    Denied,
    Info,
    Error,
    Summary,
}

struct Entry {
    kind: Kind,
    /// Czasownik w czasie teraźniejszym („Reading") albo przeszłym („Read").
    verb: &'static str,
    /// Ścieżka/wiadomość/argument — to, co pager pokazuje po czasowniku.
    subject: String,
    /// Przygaszony dopisek po nagłówku: `+12 B`, `✗ ENOENT`.
    detail: String,
    /// Treść pod nagłówkiem (podgląd wyniku, błąd, myśl).
    body: Option<String>,
    /// Zapisana treść `fs.write` — z niej rysuje się blok diffu
    /// (`scrollback/blocks/tool/edit.rs`), a nie z podglądu wyniku.
    content: Option<String>,
    step: u64,
    tool: String,
    running: bool,
    ok: bool,
    /// Runda 21: blok myśli rośnie na żywo ze strumienia providera. Po zdarzeniu
    /// `thought` (albo końcu tury) znacznik gaśnie i blok renderuje się jak dotąd.
    live: bool,
    /// Godzina zdarzenia `HH:MM` (czas lokalny) — prawa strona pasa zadania.
    stamp: String,
    /// Czas od wysłania zadania do tej myśli: `◆ Thought for 2.0s`.
    latency: Option<Duration>,
}

impl Entry {
    fn new(kind: Kind, verb: &'static str, subject: impl Into<String>) -> Entry {
        Entry {
            kind,
            verb,
            subject: subject.into(),
            detail: String::new(),
            body: None,
            content: None,
            step: 0,
            tool: String::new(),
            running: false,
            ok: true,
            live: false,
            stamp: clock_now(),
            latency: None,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Entry {
        self.detail = detail.into();
        self
    }

    fn body(mut self, body: Option<String>) -> Entry {
        self.body = body;
        self
    }

    fn step(mut self, step: u64) -> Entry {
        self.step = step;
        self
    }
}

// ── Wejście/wyjście trybu ───────────────────────────────────────────────────

/// Od którego ekranu startuje TUI: sesja (zadanie z argv), ekran powitalny
/// (`ardntai tui` bez zadania) albo lista sesji (skrót `ctrl+w`, zrzuty).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Start {
    Session,
    Welcome,
    Resume,
    Sessions,
}

pub struct Options {
    pub workspace: PathBuf,
    pub provider: Option<String>,
    pub max_steps: usize,
    pub memory: MemoryLevel,
    /// `--memory` podane w argv: wtedy flaga wygrywa z `.agent/config`.
    pub memory_set: bool,
    pub mcp: bool,
    pub task: Option<String>,
    /// Ekran startowy — patrz [`Start`].
    pub start: Start,
}

impl Options {
    pub fn new(workspace: PathBuf) -> Options {
        Options {
            workspace,
            provider: None,
            max_steps: 12,
            memory: MemoryLevel::Lite,
            memory_set: false,
            mcp: true,
            task: None,
            start: Start::Session,
        }
    }
}

/// `ardntai tui [opcje] ["zadanie"]`.
pub fn parse(args: &[String]) -> Result<Options, i32> {
    let mut opts = Options::new(std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut task: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" | "--workspace" => match args.get(i + 1) {
                Some(v) => {
                    opts.workspace = PathBuf::from(v);
                    i += 2;
                }
                None => {
                    eprintln!("--workspace wymaga sciezki");
                    return Err(2);
                }
            },
            "--provider" => match args.get(i + 1) {
                Some(v) => {
                    opts.provider = Some(v.clone());
                    i += 2;
                }
                None => {
                    eprintln!("--provider wymaga nazwy");
                    return Err(2);
                }
            },
            "--steps" => match args.get(i + 1).and_then(|v| v.parse::<usize>().ok()) {
                Some(n) if n > 0 => {
                    opts.max_steps = n;
                    i += 2;
                }
                _ => {
                    eprintln!("--steps wymaga liczby > 0");
                    return Err(2);
                }
            },
            "--memory" => match args.get(i + 1).and_then(|v| MemoryLevel::parse(v)) {
                Some(level) => {
                    opts.memory = level;
                    opts.memory_set = true;
                    i += 2;
                }
                None => {
                    eprintln!("--memory: dozwolone lite, pro, ultra");
                    return Err(2);
                }
            },
            "--mcp" => {
                opts.mcp = true;
                i += 1;
            }
            "--no-mcp" => {
                opts.mcp = false;
                i += 1;
            }
            other => {
                task.push(other.to_string());
                i += 1;
            }
        }
    }
    let joined = task.join(" ");
    if !joined.trim().is_empty() {
        opts.task = Some(joined);
    } else {
        // `ardntai tui` bez zadania = ekran powitalny (zrzut `welcome.txt`).
        opts.start = Start::Welcome;
    }
    Ok(opts)
}

pub fn run(opts: Options) -> i32 {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return run_plain(opts);
    }
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("nie mozna wystartowac TUI: {}", e);
            return 2;
        }
    };
    let mut app = App::new(opts);
    // Runda 15: mysz przechwytywana tylko przy `mouse = on` (domyślnie on).
    app.interactive = true;
    app.apply_mouse();
    let code = event_loop(&mut terminal, &mut app);
    let _ = crossterm::execute!(std::io::stdout(), DisableMouseCapture);
    let _ = ratatui::try_restore();
    code
}

/// Bez TTY nie ma czego rysować: wykonaj zadanie liniowo (albo powiedz, czego brakuje).
fn run_plain(opts: Options) -> i32 {
    let task = match opts.task.clone() {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            eprintln!("brak terminala (TTY) — tryb TUI potrzebuje interaktywnego terminala.");
            eprintln!("uzycie: arndtai tui [\"zadanie\"] [-w FOLDER] [--provider mock]");
            return 0;
        }
    };
    // Tryb bez TTY: wybór modelu z `.agent/config` (cache metadanych, jeśli
    // jest — bez sieci), `ARNDTAI_MODEL` nadal wygrywa w providerze.
    let settings = Settings::load(&opts.workspace);
    let configured = models::config_get(&opts.workspace, "model");
    let model_limit = models::read_cache(&opts.workspace)
        .and_then(|catalog| {
            let model = catalog.find(configured.as_deref()?)?;
            model.output
        });
    let max_tokens = Some(settings.token_budget(model_limit));
    let cfg = Config {
        workspace: opts.workspace,
        max_steps: opts.max_steps,
        allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
        provider: opts.provider,
        quiet: false,
        mcp: opts.mcp,
        memory: opts.memory,
        resume: false,
        model: configured,
        max_tokens,
        approver: None,
        skill_context: Vec::new(),
    };
    let report = agent::run(&task, &cfg);
    if report.status == "DONE" {
        0
    } else {
        1
    }
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> i32 {
    loop {
        app.poll();
        if app.quit {
            return 0;
        }
        let _ = terminal.draw(|f| app.draw(f));
        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => app.on_key(key),
                Ok(Event::Mouse(mouse)) => app.on_mouse(mouse.kind),
                Ok(_) => {}
                Err(_) => return 0,
            }
        }
        app.tick();
    }
}

// ── Układ ekranu (czysta funkcja rozmiaru, jak `AgentViewLayout::compute`) ──

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Screen {
    top: Rect,
    scrollback: Rect,
    turn: Rect,
    prompt: Rect,
    footer: Rect,
    /// Wiersze na podpowiedzi `/`, odebrane scrollbackowi (runda 14);
    /// puste, gdy podpowiedzi nie ma.
    hints: Rect,
    compact: bool,
}

impl Screen {
    /// Miejsce na podpowiedzi `/` bierze się ze scrollbacku, nie z treści pod
    /// composerem: composer i wiersz aktywności przesuwają się w górę o `rows`,
    /// a lista leży **pod composerem**, tuż nad stopką. `SCROLLBACK_MIN_ROWS`
    /// zostaje nietknięte.
    fn reserve_hints(&mut self, rows: u16) {
        if rows == 0 {
            return;
        }
        let room = self.scrollback.height.saturating_sub(SCROLLBACK_MIN_ROWS);
        let rows = rows.min(room);
        if rows == 0 {
            return;
        }
        self.scrollback.height -= rows;
        if self.turn.height > 0 {
            self.turn.y = self.turn.y.saturating_sub(rows);
        }
        self.prompt.y = self.prompt.y.saturating_sub(rows);
        self.hints = Rect {
            x: self.scrollback.x,
            y: self.prompt.y + self.prompt.height,
            width: self.scrollback.width,
            height: rows,
        };
    }
}

/// Prostokąt wiersza wewnątrz `inner` (ta sama szerokość, zadana wysokość).
fn rect(inner: Rect, y: u16, height: u16) -> Rect {
    Rect {
        x: inner.x,
        y,
        width: inner.width,
        height,
    }
}

/// `AgentViewLayout::compute` bez paneli, których ArndtAi nie ma (todo,
/// kolejka, dock, btw, banner): pasek (1), odstęp, scrollback (`Min(5)`),
/// ewentualny odstęp + wiersz aktywności, odstęp promptu, prompt, status (1),
/// podpowiedzi (1).
///
/// `prompt_rows` to liczba wierszy tekstu w composerze: wysokość promptu jest
/// jak w oryginale `vpad_top(1) + tekst + info_block(1)`, nie mniej niż 3
/// i nie więcej niż `min(area.height / 2, miejsce nad minimum scrollbacku)`.
fn layout(area: Rect, running: bool, prompt_rows: u16) -> Screen {
    let compact = area.height <= AUTO_COMPACT_MAX_ROWS;
    let short = area.height <= SHORT_TERMINAL_ROWS;
    let vpad = if compact { 0 } else { OUTER_VPAD };
    let hpad = if compact { COMPACT_HPAD } else { OUTER_HPAD };
    let inner = Rect {
        x: area.x.saturating_add(hpad),
        y: area.y.saturating_add(vpad),
        width: area.width.saturating_sub(hpad.saturating_mul(2)),
        height: area.height.saturating_sub(vpad.saturating_mul(2)),
    };

    // Pager: `status_gap` i `prompt_gap` istnieją tylko przy widocznym vpadzie.
    let gap = if vpad > 0 { 1 } else { 0 };
    let mut prompt_gap = if short { 0 } else { gap };
    let mut turn_total = if running { 2 } else { 0 }; // 1 wiersz odstępu + 1 wiersz aktywności
    // Za mały terminal: najpierw traci wiersz aktywności, potem odstęp promptu —
    // scrollback, prompt i stopka zostają (reguła `SCROLLBACK_MIN_ROWS`).
    let base = 2; // górny pasek + stopka
    while turn_total + prompt_gap > 0
        && base + turn_total + prompt_gap + PROMPT_HEIGHT + SCROLLBACK_MIN_ROWS > inner.height
    {
        if turn_total > 0 {
            turn_total = 0;
        } else {
            prompt_gap = 0;
        }
    }

    // Composer: `desired_height` = vpad_top + wiersze tekstu + info_block.
    let want = 2u16.saturating_add(prompt_rows.max(1));
    let room = inner
        .height
        .saturating_sub(base + turn_total + prompt_gap + SCROLLBACK_MIN_ROWS);
    let max_prompt = (area.height / 2).max(PROMPT_HEIGHT).min(room.max(PROMPT_HEIGHT));
    let prompt_h = want.min(max_prompt).max(PROMPT_HEIGHT);
    let scroll_h = inner
        .height
        .saturating_sub(base + turn_total + prompt_gap + prompt_h);

    let mut y = inner.y;
    let top = rect(inner, y, 1);
    y = y.saturating_add(1 + gap);
    let scrollback = rect(inner, y, scroll_h);
    y = y.saturating_add(scroll_h);
    let mut turn = Rect::default();
    if turn_total > 0 {
        y = y.saturating_add(1);
        turn = rect(inner, y, 1);
        y = y.saturating_add(1);
    }
    y = y.saturating_add(prompt_gap);
    let prompt = rect(inner, y, prompt_h);
    y = y.saturating_add(prompt_h);
    let footer = rect(inner, y, 1);

    Screen {
        top,
        scrollback,
        turn,
        prompt,
        footer,
        hints: Rect::default(),
        compact,
    }
}

// ── Układ ekranu startowego (`views/dashboard/layout.rs`) ───────────────────

/// Poniżej tej szerokości dashboard rysuje wiersze bez subtitle i badge'ów
/// (`MIN_DASHBOARD_WIDTH` z oryginału).
const MIN_DASHBOARD_WIDTH: u16 = 40;
/// Minimalna wysokość pasa listy przy otwartym peeku (`LIST_FLOOR_ROWS`).
const LIST_FLOOR_ROWS: u16 = 12;
/// Panel peek: minimalna wysokość całego pudełka (`PEEK_MIN_BOX_LIVE_TAIL`).
const PEEK_MIN_BOX_LIVE_TAIL: u16 = 8;
/// Sufit wysokości peeku: ⌊H × 3/8⌋ (`PEEK_MAX_FRAC_NUM` / `_DEN`).
const PEEK_MAX_FRAC_NUM: u16 = 3;
const PEEK_MAX_FRAC_DEN: u16 = 8;
/// Sufit wierszy treści w peeku (`MAX_LIVE_TAIL_ROWS`).
const MAX_LIVE_TAIL_ROWS: u16 = 28;
/// Minimalna szerokość panelu: poniżej tego panel znika, zanim lista straci
/// `MIN_DASHBOARD_WIDTH` kolumn.
const PEEK_MIN_WIDTH: u16 = 28;
const HEADER_OUTER_HPAD: u16 = 2;
const LIST_OUTER_HPAD: u16 = 2;
const DISPATCH_OUTER_HPAD: u16 = 2;
/// Wiersz: tytuł, druga linia, oddech (`ROW_HEIGHT` z `render.rs`).
const ROW_HEIGHT: u16 = 3;
/// Nagłówek grupy: etykieta i oddech (`GROUP_HEADER_HEIGHT`).
const GROUP_HEADER_HEIGHT: u16 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ListPanes {
    top_margin: Rect,
    header: Rect,
    header_gap: Rect,
    actions: Rect,
    actions_gap: Rect,
    list: Rect,
    /// Panel peek po prawej stronie listy (`views/dashboard/peek.rs`);
    /// `width == 0` znaczy „ukryty”.
    peek: Rect,
    dispatch: Rect,
    footer: Rect,
    bottom_margin: Rect,
}

/// Podział pasa listy: lista + panel peek po prawej.
/// Panel znika, **zanim** lista straci `MIN_DASHBOARD_WIDTH` kolumn, i nie
/// rośnie wyżej niż ⌊H × 3/8⌋ (`PEEK_MAX_FRAC_*`), z minimum
/// `PEEK_MIN_BOX_LIVE_TAIL`; przy niskim ekranie obowiązuje `LIST_FLOOR_ROWS`.
fn split_list_and_peek(list: Rect, screen_h: u16) -> (Rect, Rect) {
    let hidden = Rect {
        x: list.x,
        y: list.y,
        width: 0,
        height: 0,
    };
    if list.width == 0 || list.height < LIST_FLOOR_ROWS || list.height < PEEK_MIN_BOX_LIVE_TAIL {
        return (list, hidden);
    }
    let min_total = MIN_DASHBOARD_WIDTH + 1 + PEEK_MIN_WIDTH;
    if list.width < min_total {
        return (list, hidden);
    }
    let max_peek_h = (u32::from(screen_h) * u32::from(PEEK_MAX_FRAC_NUM)
        / u32::from(PEEK_MAX_FRAC_DEN)) as u16;
    let peek_h = max_peek_h.min(list.height);
    if peek_h < PEEK_MIN_BOX_LIVE_TAIL {
        return (list, hidden);
    }
    let wanted = (u32::from(list.width) * u32::from(PEEK_MAX_FRAC_NUM)
        / u32::from(PEEK_MAX_FRAC_DEN)) as u16;
    let peek_w = wanted.clamp(PEEK_MIN_WIDTH, list.width - MIN_DASHBOARD_WIDTH - 1);
    let list_w = list.width - peek_w - 1;
    let list_rect = Rect {
        width: list_w,
        ..list
    };
    let peek_rect = Rect {
        x: list.x + list_w + 1,
        y: list.y,
        width: peek_w,
        height: peek_h,
    };
    (list_rect, peek_rect)
}

/// `compute_layout` z `views/dashboard/layout.rs`: te same progi (7/5/11/12/14/17
/// wierszy), ten sam padding 2 kolumn dla nagłówka, listy i dispatchu.
fn list_layout(area: Rect) -> ListPanes {
    if area.height == 0 {
        return ListPanes::default();
    }
    let top_margin = u16::from(area.height > 6);
    let header = u16::from(area.height > 4);
    let actions = header;
    let dispatch_gap = u16::from(area.height > 10);
    let shortcuts_gap = u16::from(area.height > 11);
    let header_gap = u16::from(area.height > 12);
    let actions_gap = u16::from(area.height > 13);
    let footer = u16::from(area.height >= 2);
    let bottom_margin = u16::from(area.height > 16);
    let short_terminal = area.height <= 8;
    let overhead = top_margin
        + header
        + header_gap
        + actions
        + actions_gap
        + footer
        + dispatch_gap
        + shortcuts_gap
        + bottom_margin;
    let dispatch_h = if short_terminal { 1 } else { 3 };
    let dispatch_h = dispatch_h.min(area.height.saturating_sub(overhead));
    let remaining = area.height.saturating_sub(overhead + dispatch_h);

    let inset = |pad: u16| -> (u16, u16) {
        let inner_pad = pad.saturating_mul(2);
        let width = area.width.saturating_sub(inner_pad);
        if width > 0 {
            (area.x.saturating_add(pad), width)
        } else {
            (area.x, area.width)
        }
    };
    let (header_x, header_w) = inset(HEADER_OUTER_HPAD);
    let (list_x, list_w) = inset(LIST_OUTER_HPAD);
    let (dispatch_x, dispatch_w) = inset(DISPATCH_OUTER_HPAD);

    let mut y = area.y;
    let top_margin_rect = rect(area, y, top_margin);
    y += top_margin;
    let header_rect = Rect {
        x: header_x,
        y,
        width: header_w,
        height: header,
    };
    y += header;
    let header_gap_rect = rect(area, y, header_gap);
    y += header_gap;
    let actions_rect = Rect {
        x: header_x,
        y,
        width: header_w,
        height: actions,
    };
    y += actions;
    let actions_gap_rect = rect(area, y, actions_gap);
    y += actions_gap;
    let list_band = Rect {
        x: list_x,
        y,
        width: list_w,
        height: remaining,
    };
    let (list_rect, peek_rect) = split_list_and_peek(list_band, area.height);
    y += remaining;
    y += dispatch_gap;
    let dispatch_rect = Rect {
        x: dispatch_x,
        y,
        width: dispatch_w,
        height: dispatch_h,
    };
    y += dispatch_h;
    y += shortcuts_gap;
    let footer_rect = rect(area, y, footer);
    y += footer;
    let bottom_margin_rect = rect(area, y, bottom_margin);

    ListPanes {
        top_margin: top_margin_rect,
        header: header_rect,
        header_gap: header_gap_rect,
        actions: actions_rect,
        actions_gap: actions_gap_rect,
        list: list_rect,
        peek: peek_rect,
        dispatch: dispatch_rect,
        footer: footer_rect,
        bottom_margin: bottom_margin_rect,
    }
}

// ── Stan ekranu ─────────────────────────────────────────────────────────────

enum Msg {
    Done(agent::Report),
}

/// Który ekran widzi użytkownik — odpowiednik dashboardu i widoku agenta.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    /// Ekran powitalny: logo, menu, composer (`welcome.txt`).
    Welcome,
    /// Modal „Resume session" (`resume-modal.txt`) — wejście do sesji.
    Resume,
    /// Lista sesji (`views/dashboard`).
    Sessions,
    /// Sesja: scrollback, composer, pasek stanu (`views/agent.rs`).
    Session,
}

/// Stan wyszukiwania w scrollbacku (`Ctrl+F`, runda 17).
struct Search {
    query: String,
    /// Zaznaczone trafienie (0 = pierwsze). Trafienia liczone są na bieżąco
    /// po szerokości z ostatniego rysowania, więc zmiana rozmiaru okna nie
    /// zostawia listy sprzed resize'u.
    current: usize,
    /// Miejsce, z którego przyszło wyszukiwanie — `Esc` tu wraca.
    saved_scroll: usize,
    /// `true` dopóki znaki idą do zapytania; `Enter` przechodzi w tryb `n`/`N`.
    editing: bool,
    /// `true` gdy modal jest na ekranie; po `Enter` znika, a trafienia
    /// podświetlają się w scrollbacku (stopka pokazuje licznik).
    visible: bool,
}

pub struct App {
    workspace: PathBuf,
    provider: Option<String>,
    max_steps: usize,
    memory: MemoryLevel,
    mcp: bool,
    /// Sam model do podpisu composera (`<model> · <tryb>`).
    model: String,
    /// Tryb zgód z konfiguracji narzędzi (`always-approve` / `ask`).
    mode: String,
    /// Metadane z `/models`: limit kontekstu i lista do pickera (runda 14).
    catalog: Catalog,
    /// Realne tokeny z ostatniego `usage`; `None` → liczone z znaków z `~`.
    context_tokens: Option<u64>,
    /// Otwarty picker `/model`: indeks zaznaczenia w `catalog.models`.
    picker: Option<usize>,
    /// Podpowiedzi `/` schowane `Esc`-em do następnej zmiany tekstu.
    hints_hidden: bool,
    hints_sel: usize,
    /// Model przełączony przed pierwszym zadaniem — `note` czeka na sesję.
    pending_note: Option<String>,

    entries: Vec<Entry>,
    input: String,
    cursor: usize,
    history: Vec<String>,
    hist_pos: Option<usize>,
    scroll: usize,

    running: bool,
    status: Option<String>,
    steps: usize,
    tool_calls: usize,
    denied: usize,
    drift: usize,
    session: Option<String>,
    turn_started: Option<Instant>,
    /// Czy w tej turze była już myśl: `for Xs` liczy się od wysłania do
    /// pierwszej treści, więc kolejne myśli dostają samo `◆ Thought`.
    turn_first_thought: bool,
    /// Znaki transkryptu sesji (licznik kontekstu w górnym pasku).
    context_chars: usize,
    /// Znaki zdarzeń bieżącej tury (prawa strona wiersza `Responding…`).
    turn_chars: usize,

    rx: Option<Receiver<Msg>>,
    events: Option<Receiver<Value>>,
    /// Runda 21: przyrosty strumienia (poza transkryptem) — blok myśli rośnie
    /// między zdarzeniami sesji, bez dopisywania linii do JSONL.
    deltas: Option<Receiver<provider::Delta>>,
    tick_count: u64,
    quit: bool,
    toast: Option<String>,

    // ── Ekran startowy (`views/dashboard`) ──
    view: View,
    sessions: Vec<SessionRow>,
    selected: usize,
    list_offset: usize,
    filter: Filter,
    /// Szukanie w modalu „Resume session": `/` włącza, tekst zawęża tytuły.
    query: String,
    searching: bool,
    /// Sesja otwarta z listy albo po przebiegu: kolejne zadanie ją wznawia
    /// (ten sam `session_id` i plik transkryptu), zamiast zakładać nową.
    session_title: Option<String>,
    /// Ostatnio wysłane zadanie. Po `FAILED_*` wraca do composera, żeby nie
    /// przepisywać go od nowa (runda 13).
    last_task: Option<String>,

    // ── Runda 15: ustawienia, tryb `ask`, motyw ──
    /// Sześć kluczy z `.agent/config` + ostrzeżenia z walidacji.
    settings: Settings,
    /// Otwarty modal `/settings`: indeks zaznaczonego wiersza.
    settings_sel: Option<usize>,
    /// Pytanie trybu `ask` czekające na `t`/`n` w composerze.
    question: Option<agent::ApprovalRequest>,
    /// Koniec TUI bramki zgód (tylko gdy tura leci w trybie `ask`).
    approval: Option<agent::ApprovalHandle>,
    /// Ostrzeżenia z odczytu configu — wracają po otwarciu nowej sesji.
    startup_warnings: Vec<String>,

    // ── Runda 16: skille ──
    /// Odkryte skille: workspace → katalog aplikacji → `~/.config/arndtai`.
    skills: Vec<crate::skills::Skill>,
    /// Otwarty modal `/skills`: indeks zaznaczonego wiersza.
    skills_sel: Option<usize>,
    /// Nazwy skilli w kontekście — z `/skill` albo z `skill.load` modelu.
    loaded_skills: Vec<String>,
    /// `true` tylko gdy działa prawdziwy terminal (`run`); zrzuty i testy
    /// rysują do bufora i nie mogą wołać `execute!`.
    interactive: bool,

    // ── Runda 17: szukanie w scrollbacku ──
    /// Otwarty modal `Ctrl+F`: zapytanie, trafienia, zaznaczenie.
    search: Option<Search>,
    /// Szerokość treści i wysokość okna z ostatniego rysowania — trafienia
    /// liczone są po tych samych wierszach, które widać na ekranie.
    search_width: Cell<usize>,
    search_view: Cell<usize>,

    // ── Runda 18: markdown w odpowiedziach asystenta ──
    /// Wyrenderowane bloki asystenta, klucz (tekst, szerokość). Streaming
    /// zmienia tylko ostatni blok, więc reszta zostaje bez ponownego parsowania.
    md: markdown::Cache,
}

impl App {
    pub fn new(opts: Options) -> App {
        let brain = Brain::from_env_full(opts.provider.as_deref(), "", &[], None, None);
        let conn = brain.conn();
        // Jedno pobranie na start procesu; dalej cache `.agent/models.json`.
        let catalog = models::load(&opts.workspace, conn.as_ref());
        // Runda 15: ustawienia z `.agent/config`; `--memory` z argv wygrywa.
        let loaded = Settings::load(&opts.workspace);
        let memory = if opts.memory_set {
            opts.memory
        } else {
            loaded.memory
        };
        let settings = Settings {
            memory,
            ..loaded
        };
        // Offline nie ma listy modeli — podpis zostaje „offline (mock)".
        let model = if matches!(&brain, Brain::Mock(_)) {
            brain.model_name()
        } else {
            models::resolve_model(
                &opts.workspace,
                models::env_model().as_deref(),
                &brain.model_name(),
            )
        };
        let view = match opts.start {
            Start::Welcome => View::Welcome,
            Start::Resume => View::Resume,
            Start::Sessions => View::Sessions,
            Start::Session => View::Session,
        };
        let sessions = if matches!(view, View::Welcome | View::Sessions | View::Resume) {
            sessions::scan(&opts.workspace)
        } else {
            Vec::new()
        };
        // Proaktywność: jeden przebieg reguł na start (bez modelu, bez sieci).
        // Trafienia (także shadow) lądują w `.agent/policies.log`, a notatki
        // w `.agent-memory/notes.md` — TUI ich nie rysuje, żeby nie mylić ich
        // z rozmową; widzi je `ardntai tick` i pamięć kolejnych zadań.
        let _ = crate::proactive::tick(&opts.workspace, crate::context::now_ts());
        let startup_warnings = settings.warnings.clone();
        // Runda 16: skille odkryte raz na start; `/skills` odświeża listę.
        let skills = crate::skills::discover(&opts.workspace);
        let mut app = App {
            workspace: opts.workspace,
            provider: opts.provider,
            max_steps: opts.max_steps,
            memory: settings.memory.clone(),
            mcp: opts.mcp,
            model,
            mode: settings.approval.as_str().to_string(),
            catalog,
            context_tokens: None,
            picker: None,
            hints_hidden: false,
            hints_sel: 0,
            pending_note: None,
            entries: Vec::new(),
            input: String::new(),
            cursor: 0,
            history: Vec::new(),
            hist_pos: None,
            scroll: 0,
            running: false,
            status: None,
            steps: 0,
            tool_calls: 0,
            denied: 0,
            drift: 0,
            session: None,
            turn_started: None,
            turn_first_thought: true,
            context_chars: 0,
            turn_chars: 0,
            rx: None,
            events: None,
            deltas: None,
            tick_count: 0,
            quit: false,
            toast: None,
            view,
            sessions,
            selected: 0,
            list_offset: 0,
            filter: Filter::All,
            query: String::new(),
            searching: false,
            session_title: None,
            last_task: None,
            settings,
            settings_sel: None,
            question: None,
            approval: None,
            startup_warnings,
            skills,
            skills_sel: None,
            loaded_skills: Vec::new(),
            interactive: false,
            search: None,
            search_width: Cell::new(119),
            search_view: Cell::new(20),
            md: markdown::Cache::new(),
        };
        // Ostrzeżenia z walidacji `.agent/config` idą do scrollbacku od razu;
        // nie zatrzymują programu (nieznana wartość = wartość domyślna).
        app.push_startup_warnings();
        app
    }

    /// Ostrzeżenia z odczytu configu — raz na sesję, po wyczyszczeniu ekranu.
    fn push_startup_warnings(&mut self) {
        for warning in self.startup_warnings.clone() {
            self.push(Entry::new(Kind::Info, "", warning));
        }
    }

    /// Ponowne czytanie transkryptów — lista zawsze pokazuje to, co jest na dysku.
    pub fn refresh_sessions(&mut self) {
        self.sessions = sessions::scan(&self.workspace);
        self.clamp_selection();
    }

    /// Runda 16: ponowne odkrycie skilli (modal i `/skill` czytają z dysku).
    /// Wczytane nazwy, których już nie ma, wypadają — kontekst nie trzyma
    /// treści skilla, którego ktoś usunął.
    fn refresh_skills(&mut self) {
        self.skills = crate::skills::discover(&self.workspace);
        if let Some(selected) = self.skills_sel {
            if selected >= self.skills.len() {
                self.skills_sel = Some(self.skills.len().saturating_sub(1));
            }
        }
        self.loaded_skills
            .retain(|name| crate::skills::find(&self.skills, name).is_some());
    }

    /// Treść skilli wczytanych `/skill` — jedyne miejsce, w którym pełny tekst
    /// idzie do kontekstu modelu (obok narzędzia `skill.load`).
    fn skill_context(&self) -> Vec<(String, String)> {
        self.loaded_skills
            .iter()
            .filter_map(|name| {
                crate::skills::find(&self.skills, name)
                    .map(|skill| (skill.name.clone(), skill.body.clone()))
            })
            .collect()
    }

    fn visible_sessions(&self) -> Vec<&SessionRow> {
        self.sessions
            .iter()
            .filter(|row| self.filter.matches(row))
            .collect()
    }

    /// Sesje w modalu: filtr stanu plus szukanie po tytule (`/`).
    fn resume_sessions(&self) -> Vec<&SessionRow> {
        let query = self.query.to_lowercase();
        self.sessions
            .iter()
            .filter(|row| self.filter.matches(row))
            .filter(|row| query.is_empty() || row.title.to_lowercase().contains(&query))
            .collect()
    }

    /// Wiersze, po których chodzi zaznaczenie na bieżącym ekranie.
    fn cursor_rows(&self) -> Vec<&SessionRow> {
        if self.view == View::Resume {
            self.resume_sessions()
        } else {
            self.visible_sessions()
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.cursor_rows().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn config(&self) -> Config {
        Config {
            workspace: self.workspace.clone(),
            max_steps: self.max_steps,
            allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
            provider: self.provider.clone(),
            quiet: true,
            mcp: self.mcp,
            memory: self.memory.clone(),
            // Otwarta sesja = to samo `session_id` i ten sam plik transkryptu.
            resume: self.session.is_some(),
            // Wybór z `/model` (albo `.agent/config`) idzie do providera;
            // `ARNDTAI_MODEL` nadal wygrywa w `Brain::from_env_full`.
            model: Some(self.model.clone()),
            max_tokens: self.max_tokens(),
            // Bramkę zgód dostaje tylko tura w trybie `ask` — dokłada ją
            // `submit`, bo potrzebuje kanałów do UI.
            approver: None,
            // Runda 16: tylko skille wczytane `/skill` — lista idzie osobno,
            // do promptu systemowego (agent robi to sam).
            skill_context: self.skill_context(),
        }
    }

    /// Limit odpowiedzi: ustawienie z `/settings` przycięte do `limit.output`
    /// wybranego modelu. Runda 14 brała samo `limit.output`.
    fn max_tokens(&self) -> Option<usize> {
        let limit = self.catalog.find(&self.model).and_then(|m| m.output);
        Some(self.settings.token_budget(limit))
    }

    pub fn tick(&mut self) {
        self.tick_count = self.tick_count.saturating_add(1);
    }

    /// Zrzut ekranu sesji pokazuje **poczatek** przebiegu: pas zadania i
    /// odstepy musza byc w kadrze, a przy dlugim transkrypcie dolna strona
    /// scrollbacku je wypycha. Zwykly start dalej otwiera ostatnia strone.
    pub fn show_scrollback_start(&mut self) {
        self.scroll = usize::MAX / 2;
    }

    fn spinner(&self) -> &'static str {
        SPINNER[(self.tick_count / SPINNER_DIVISOR) as usize % SPINNER.len()]
    }

    fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
        if self.scroll > 0 {
            // Nowy wpis nie może „uciekać" spod przewiniętego widoku.
            self.scroll = self.scroll.saturating_add(2);
        }
    }

    // ── Zdarzenia ───────────────────────────────────────────────────────────

    /// Runda 21: przyrost myślenia dopisuje się do **ostatniego** bloku
    /// `Thought` oznaczonego jako `live`; gdy go nie ma (nowy krok albo
    /// pierwszy przyrost), powstaje jeden nowy blok. Kanał `content` to JSON
    /// decyzji — nie pokazujemy go w scrollbacku, bo po kroku przychodzi
    /// normalne zdarzenie `thought` i render markdown.
    fn apply_delta(&mut self, delta: provider::Delta) {
        if delta.text.is_empty() {
            return;
        }
        self.turn_chars = self.turn_chars.saturating_add(delta.text.len());
        if delta.channel != provider::Channel::Reasoning {
            return;
        }
        let step = delta.step;
        let grows = self.entries.last().is_some_and(|last| {
            last.kind == Kind::Thought && last.live && (last.step == step || last.step == 0)
        });
        if grows {
            if let Some(last) = self.entries.last_mut() {
                last.subject.push_str(&delta.text);
                if last.step == 0 {
                    last.step = step;
                }
            }
        } else {
            let latency = if self.turn_first_thought {
                self.turn_started.map(|started| started.elapsed())
            } else {
                None
            };
            self.turn_first_thought = false;
            let mut entry = Entry::new(Kind::Thought, "", delta.text).step(step);
            entry.live = true;
            entry.latency = latency;
            self.push(entry);
        }
        if (step as usize) > self.steps {
            self.steps = step as usize;
        }
    }

    pub fn poll(&mut self) {
        // Runda 21: kolejność w obrębie jednego `poll` musi być jak w sesji:
        // najpierw `task` (pas zadania nad blokami), potem przyrosty strumienia,
        // na końcu reszta zdarzeń — dopiero wtedy `thought` domyka pełny blok.
        // Oba kanały są niezależne, więc bez tego porządku przy szybkim SSE
        // myśl mogłaby wylądować nad pasem zadania.
        let mut batch: Vec<Value> = Vec::new();
        if let Some(rx) = &self.events {
            while let Ok(value) = rx.try_recv() {
                batch.push(value);
            }
        }
        let mut deferred: Vec<Value> = Vec::new();
        for value in batch {
            let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            if kind == "task" {
                self.handle_event(value);
            } else {
                deferred.push(value);
            }
        }
        let mut deltas: Vec<provider::Delta> = Vec::new();
        if let Some(rx) = &self.deltas {
            while let Ok(delta) = rx.try_recv() {
                deltas.push(delta);
            }
        }
        for delta in deltas {
            self.apply_delta(delta);
        }
        for value in deferred {
            self.handle_event(value);
        }
        // Runda 15: pytanie trybu `ask`. Agent stoi na bramce, dopóki nie
        // odpowiemy — dlatego pytanie trafia do composera, a nie do scrollbacku.
        if let Some(handle) = &self.approval {
            if self.question.is_none() {
                self.question = handle.try_recv();
            }
        }
        // Realne tokeny: provider przysłał `usage` w odpowiedzi (runda 14).
        // Bez tego licznik zostaje na szacunku z znaków z `~`.
        if self.running {
            if let Some(usage) = crate::provider::last_usage() {
                self.context_tokens = Some(usage.prompt.saturating_add(usage.completion));
            }
        }

        let mut finished: Option<agent::Report> = None;
        if let Some(rx) = &self.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Msg::Done(report) => finished = Some(report),
                }
            }
        }
        if let Some(report) = finished {
            self.finish_report(report);
        }
    }

    /// Zamkniecie tury: status, liczniki i powrot tekstu do composera.
    ///
    /// Runda 13: gdy perimetr nie wystartowal (`FAILED_ERROR`), zadanie wraca do
    /// pola wpisywania — inaczej trzeba by je pisac od nowa. Tekstu, ktory
    /// uzytkownik zdazyl wpisac po wyslaniu, nie nadpisujemy.
    fn finish_report(&mut self, report: agent::Report) {
        self.running = false;
        self.turn_started = None;
        self.status = Some(report.status.clone());
        self.steps = report.steps.max(self.steps);
        self.tool_calls = report.tool_calls.max(self.tool_calls);
        self.denied = report.denied.max(self.denied);
        self.drift = report.drift;
        self.session = report
            .session
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        // Model wybrany przed pierwszym zadaniem: sesja już istnieje, więc
        // `note` trafia do jej transkryptu (bez duplikatu w scrollbacku).
        if let Some(model) = self.pending_note.take() {
            if let Some(id) = self.session.clone() {
                let session = context::Session::resume(&self.workspace, &id);
                session.event("note", serde_json::json!({ "model": model }));
            }
        }
        // Pamięć `ultra`: po udanym zadaniu jedno tanie wywołanie modelu na
        // fakty (w tle, żeby nie blokować UI). Offline nic nie robi.
        if report.status == "DONE" && matches!(self.memory, crate::context::MemoryLevel::Ultra) {
            if let Some(task) = self.last_task.clone() {
                let workspace = self.workspace.clone();
                let provider = self.provider.clone();
                let reason = report.reason.clone();
                std::thread::spawn(move || {
                    if let Ok(facts) =
                        crate::provider::extract_facts(provider.as_deref(), &task, &reason)
                    {
                        crate::append_facts(&workspace, &facts);
                    }
                });
            }
        }
        self.rx = None;
        self.events = None;
        self.deltas = None;
        self.toast = None;
        context::set_event_sink(None);
        provider::set_delta_sink(None);
        // Tura może się skończyć błędem przed `thought` — żaden blok nie może
        // zostać z migającym znacznikiem „live”.
        for entry in self.entries.iter_mut() {
            entry.live = false;
        }
        // Runda 15: po zamknięciu tury nie ma na co czekać. Otwarte pytanie
        // (np. po błędzie perimetru) kończy się odmową, żeby wątek nie wisiał.
        if self.question.take().is_some() {
            if let Some(handle) = &self.approval {
                handle.answer(false);
            }
        }
        self.approval = None;

        if report.status.starts_with("FAILED") && self.input.trim().is_empty() {
            if let Some(task) = self.last_task.clone() {
                self.input = task;
                self.cursor = self.input.chars().count();
                self.hist_pos = None;
                self.scroll = 0;
            }
        }
    }

    fn handle_event(&mut self, event: Value) {
        // Licznik kontekstu: znaki transkryptu (JSONL), nie tokeny — provider
        // nie zwraca `usage`. Ta sama liczba zasila prawą stronę `Responding…`.
        let size = serde_json::to_string(&event).map(|s| s.len()).unwrap_or(0);
        self.context_chars = self.context_chars.saturating_add(size);
        if self.running {
            self.turn_chars = self.turn_chars.saturating_add(size);
        }
        let kind = event
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let data = event.get("data").cloned().unwrap_or(Value::Null);

        match kind.as_str() {
            "task" => {
                self.scroll = 0;
                let text = str_field(&data, "text");
                self.push(Entry::new(Kind::Task, "", text));
            }
            "thought" => {
                let step = num_field(&data, "step");
                self.steps = self.steps.max(step as usize);
                let thought = str_field(&data, "thought");
                // Czas „Thought for N" liczy się od wysłania zadania do tej myśli;
                // przy odtwarzaniu transkryptu (bez zegara) zostaje samo `Thought`.
                let latency = if self.turn_first_thought {
                    self.turn_started.map(|t| t.elapsed())
                } else {
                    None
                };
                self.turn_first_thought = false;
                // Runda 21: blok rósł na żywo ze strumienia — zamykamy go
                // w miejscu (bez drugiego wpisu i bez przeskoku scrollbacku).
                // Zachowujemy pełne myślenie; puste domykamy treścią zdarzenia.
                if let Some(last) = self.entries.last_mut() {
                    if last.kind == Kind::Thought && last.live {
                        last.live = false;
                        if last.step == step || last.step == 0 {
                            last.step = step;
                            if last.subject.trim().is_empty() {
                                last.subject = thought;
                            }
                            if last.latency.is_none() {
                                last.latency = latency;
                            }
                            return;
                        }
                    }
                }
                let mut entry = Entry::new(Kind::Thought, "", thought).step(step);
                entry.latency = latency;
                // Runda 18: gdy model dopisuje dalszy ciąg tej samej myśli
                // (ten sam krok, tekst zaczyna się od poprzedniego), blok rośnie
                // w miejscu — bez nowego wpisu i bez przeskoku scrollbacku.
                let grown = self.entries.last().is_some_and(|last| {
                    last.kind == Kind::Thought
                        && last.step == step
                        && entry.subject.starts_with(&last.subject)
                        && entry.subject.len() > last.subject.len()
                });
                if grown {
                    if let Some(last) = self.entries.last_mut() {
                        last.subject = entry.subject;
                        if entry.latency.is_some() {
                            last.latency = entry.latency;
                        }
                    }
                } else {
                    self.push(entry);
                }
            }
            "tool_call" => {
                let step = num_field(&data, "step");
                let tool = str_field(&data, "tool");
                let args = data.get("args").cloned().unwrap_or(Value::Null);
                let display = tool_display(&tool, &args);
                let mut entry = Entry::new(Kind::Call, display.verb_present, display.subject)
                    .detail(display.detail)
                    .step(step);
                entry.tool = tool.clone();
                entry.running = true;
                // Runda 16: gdy model sam wczytał skill, modal pokazuje go jako
                // „w kontekście" — to ta sama treść, którą ma w historii.
                if tool == "skill.load" {
                    let name = str_field(&args, "name");
                    if !name.is_empty() && !self.loaded_skills.iter().any(|n| n == &name) {
                        self.loaded_skills.push(name);
                    }
                }
                if tool == "fs.write" {
                    entry.content = data
                        .get("args")
                        .and_then(|a| a.get("content"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                self.push(entry);
            }
            "tool_result" => {
                let step = num_field(&data, "step");
                let tool = str_field(&data, "tool");
                let ok = data.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let code = str_field(&data, "code");
                let bytes = num_field(&data, "bytes");
                let output = data.get("output").cloned().unwrap_or(Value::Null);
                let slot = self.finish_call(step, &tool);
                self.tool_calls += 1;
                let subject = slot
                    .as_ref()
                    .map(|d| d.subject.clone())
                    .unwrap_or_else(|| tool.clone());
                let mut entry = if ok {
                    Entry::new(Kind::Ok, past_verb(&tool), subject)
                        .detail(format!("+{} B", bytes))
                } else {
                    Entry::new(Kind::Fail, past_verb(&tool), subject).detail(if code.is_empty() {
                        "✗ ERROR".to_string()
                    } else {
                        format!("✗ {}", code)
                    })
                };
                entry.tool = tool.clone();
                entry.ok = ok;
                entry.step = step;
                entry.content = slot.as_ref().and_then(|d| d.content.clone());
                entry.body = preview(&output, if ok { 4 } else { 6 });
                self.replace_or_push(slot.map(|d| d.slot), entry);
            }
            "tool_result_cached" => {
                let tool = str_field(&data, "tool");
                let step = num_field(&data, "step");
                let slot = self.finish_call(step, &tool);
                self.tool_calls += 1;
                let mut entry = Entry::new(
                    Kind::Cached,
                    past_verb(&tool),
                    slot.as_ref()
                        .map(|d| d.subject.clone())
                        .unwrap_or_else(|| tool.clone()),
                )
                .detail("z zapisu")
                .step(step);
                entry.tool = tool;
                self.replace_or_push(slot.map(|d| d.slot), entry);
            }
            "tool_result_skipped" => {
                let tool = str_field(&data, "tool");
                let reason = str_field(&data, "reason");
                let step = num_field(&data, "step");
                let slot = self.finish_call(step, &tool);
                let mut entry = Entry::new(
                    Kind::Skipped,
                    past_verb(&tool),
                    slot.as_ref()
                        .map(|d| d.subject.clone())
                        .unwrap_or_else(|| tool.clone()),
                )
                .detail(format!("pominiety ({})", reason))
                .step(step);
                entry.tool = tool;
                entry.ok = false;
                self.replace_or_push(slot.map(|d| d.slot), entry);
            }
            // Kontrakt v2: przejscia bramek sa telemetria, nie rozmowa.
            // TUI ich nie rysuje (widzi je tylko web UI i transkrypt).
            "gate" => {}
            "denied" => {
                let step = num_field(&data, "step");
                let tool = str_field(&data, "tool");
                let reason = str_field(&data, "reason");
                let slot = self.finish_call(step, &tool);
                self.denied += 1;
                let mut entry = Entry::new(Kind::Denied, "", format!("{} — {}", tool, reason))
                    .step(step);
                entry.tool = tool;
                entry.ok = false;
                self.replace_or_push(slot.map(|d| d.slot), entry);
            }
            "summary" => {
                let status = str_field(&data, "status");
                self.status = Some(status.clone());
                self.steps = num_field(&data, "steps") as usize;
                self.tool_calls = num_field(&data, "tool_calls") as usize;
                self.denied = num_field(&data, "denied") as usize;
                self.drift = num_field(&data, "drift") as usize;
                let reason = str_field(&data, "reason");
                let mut entry = Entry::new(Kind::Summary, "", status.clone()).body(
                    if reason.is_empty() {
                        None
                    } else {
                        Some(reason)
                    },
                );
                entry.ok = status == "DONE";
                entry.detail = format!(
                    "kroki {} · narzedzia {} · skille {} · odmowy {} · dryf {}",
                    self.steps,
                    self.tool_calls,
                    self.skills.len(),
                    self.denied,
                    self.drift
                );
                self.push(entry);
            }
            "note" => {
                // Runda 14: zmiana modelu w sesji. Kontrakt dostaje nowy
                // `kind`, ale bez zmiany istniejących pól — konsumenci mogą
                // go ignorować (jak `repair`).
                if let Some(model) = data.get("model").and_then(|v| v.as_str()) {
                    self.push(Entry::new(Kind::Info, "", format!("model: {}", model)));
                }
            }
            "store" => {
                let journal = str_field(&data, "journal_mode");
                let busy = num_field(&data, "busy_timeout");
                self.push(Entry::new(
                    Kind::Info,
                    "",
                    format!("magazyn: {} · busy_timeout {} ms", journal, busy),
                ));
            }
            "store_error" => {
                let stage = str_field(&data, "stage");
                let message = str_field(&data, "message");
                self.push(Entry::new(
                    Kind::Error,
                    "",
                    format!("magazyn ({}): {}", stage, message),
                ));
            }
            "error" => {
                let message = str_field(&data, "message");
                self.push(Entry::new(Kind::Error, "", message));
            }
            other if !other.is_empty() => {
                self.push(Entry::new(
                    Kind::Info,
                    "",
                    format!("{} {}", other, compact(&data, 100)),
                ));
            }
            _ => {}
        }
    }

    /// Wpis `tool_call` przestaje się kręcić, gdy przyszedł wynik/odmowa.
    /// Zwraca jego indeks i opis, żeby wynik zajął to samo miejsce (pager
    /// zmienia blok w miejscu, nie dopisuje drugiego).
    fn finish_call(&mut self, step: u64, tool: &str) -> Option<ToolSlot> {
        for (slot, entry) in self.entries.iter_mut().enumerate().rev() {
            if entry.kind == Kind::Call && entry.running {
                if entry.step == step && (entry.tool == tool || tool.is_empty()) {
                    entry.running = false;
                    return Some(ToolSlot {
                        slot,
                        subject: entry.subject.clone(),
                        content: entry.content.clone(),
                    });
                }
            }
        }
        None
    }

    /// Podmienia wpis wywołania na wynik; bez miejsca — dopisuje nowy.
    fn replace_or_push(&mut self, slot: Option<usize>, entry: Entry) {
        match slot.and_then(|i| self.entries.get_mut(i).map(|old| (i, old))) {
            Some((i, old)) => {
                if self.scroll > 0 {
                    // Wpis zmienia wysokość (nagłówek + wynik), więc widok nie skacze.
                    let grew = entry.body.is_some() && old.body.is_none();
                    if grew {
                        self.scroll = self.scroll.saturating_add(2);
                    }
                }
                self.entries[i] = entry;
            }
            None => self.push(entry),
        }
    }

    // ── Komendy `/` (runda 14) ─────────────────────────────────────────────

    /// Tokeny kontekstu: realne z `usage` albo szacunek z znaków (`~`).
    fn context_used(&self) -> (u64, bool) {
        match self.context_tokens {
            Some(tokens) => (tokens, false),
            None => {
                let chars = self.context_chars as u64;
                let tokens = chars.div_ceil(CHARS_PER_TOKEN);
                (tokens, true)
            }
        }
    }

    /// Podpowiedzi pod composerem: prefiks `/…` filtruje rejestr komend.
    fn hints(&self) -> Vec<&'static commands::Command> {
        if self.hints_hidden {
            return Vec::new();
        }
        commands::matches(&self.input)
    }

    /// `Tab`/`Enter` przy widocznych podpowiedziach: wstaw nazwę komendy.
    fn complete_hint(&mut self) -> bool {
        let pick = {
            let hints = self.hints();
            if hints.is_empty() {
                return false;
            }
            hints[self.hints_sel.min(hints.len() - 1)]
        };
        self.input = pick.name.to_string();
        self.cursor = self.input.chars().count();
        self.hist_pos = None;
        true
    }

    fn open_picker(&mut self) {
        if self.catalog.is_empty() {
            self.push(Entry::new(
                Kind::Info,
                "",
                format!(
                    "brak listy modeli z /models \u{2014} zostaje {}; sprawdz klucz albo siec",
                    self.model
                ),
            ));
            return;
        }
        self.picker = Some(self.catalog.active_index(&self.model));
        self.hints_hidden = true;
    }

    /// `Enter` w pickerze: model w bieżącej sesji + zapis w `.agent/config`.
    /// Zmiana dopisuje do transkryptu `note` z `{"model": "<id>"}`.
    fn select_model(&mut self, index: usize) {
        let Some(model) = self.catalog.models.get(index).cloned() else {
            return;
        };
        self.model = model.id.clone();
        if let Err(e) = models::config_set(&self.workspace, "model", &self.model) {
            self.toast = Some(e);
        }
        self.note_model();
        self.picker = None;
        self.toast = Some(format!("model: {}", self.model));
    }

    /// `note` w transkrypcie sesji; bez sesji — czeka na pierwsze zadanie.
    fn note_model(&mut self) {
        let Some(id) = self.session.clone() else {
            self.pending_note = Some(self.model.clone());
            return;
        };
        let session = context::Session::resume(&self.workspace, &id);
        session.event("note", serde_json::json!({ "model": self.model }));
        self.push(Entry::new(Kind::Info, "", format!("model: {}", self.model)));
    }

    /// Wpisane `/…` to komenda z rejestru, nie zadanie dla pętli.
    /// Zwraca `true`, gdy wejście zostało obsłużone.
    fn dispatch(&mut self) -> bool {
        let text = self.input.trim().to_string();
        if !text.starts_with('/') {
            return false;
        }
        self.input.clear();
        self.cursor = 0;
        self.history.push(text.clone());
        self.hist_pos = None;
        self.hints_hidden = false;
        self.hints_sel = 0;
        match commands::exact(&text) {
            Some(command) => {
                // `/theme jasny` i `/skill <nazwa>` mają argument — rejestr
                // zna tylko nazwę komendy.
                if matches!(command.handler, Handler::Theme) {
                    self.run_theme(&text);
                } else if matches!(command.handler, Handler::Skill) {
                    self.run_skill(&text);
                } else if matches!(command.handler, Handler::Copy) {
                    // `/copy` bez argumentu, `/copy all` — cała rozmowa.
                    let arg = text
                        .split_once(char::is_whitespace)
                        .map(|(_, rest)| rest.trim().to_string());
                    self.copy_block(arg.as_deref() == Some("all"));
                } else if matches!(command.handler, Handler::Export) {
                    // Reszta po nazwie komendy to ścieżka (może mieć spacje).
                    let path = text
                        .split_once(char::is_whitespace)
                        .map(|(_, rest)| rest.trim().to_string())
                        .filter(|rest| !rest.is_empty());
                    self.export(path.as_deref());
                } else {
                    self.run_command(command.handler);
                }
            }
            None => {
                let name = text.split_whitespace().next().unwrap_or("/").to_string();
                self.toast = Some(format!("nieznana komenda {}", name));
                self.push(Entry::new(
                    Kind::Info,
                    "",
                    format!("nieznana komenda {} \u{2014} wpisz /help", name),
                ));
            }
        }
        true
    }

    fn run_command(&mut self, handler: Handler) {
        // Komenda z ekranu powitalnego otwiera świeżą sesję: scrollback i
        // composer są tylko tam (poza `/resume` i `/quit`, które działają wszędzie).
        if self.view != View::Session && !matches!(handler, Handler::Resume | Handler::Quit) {
            self.open_new_session();
        }
        match handler {
            Handler::Help => {
                self.push(Entry::new(Kind::Info, "", commands::help_text()));
                self.scroll = 0;
            }
            Handler::Model => self.open_picker(),
            Handler::Settings => self.open_settings(),
            // `/theme` obsługuje `dispatch` (potrzebuje argumentu).
            Handler::Theme => {}
            Handler::Clear => {
                self.entries.clear();
                self.scroll = 0;
            }
            Handler::Resume => self.open_resume(),
            Handler::Skills => self.open_skills(),
            // `/skill` obsługuje `dispatch` (potrzebuje argumentu).
            Handler::Skill => {}
            // `/copy` i `/export` obsługuje `dispatch` (argumenty).
            Handler::Copy | Handler::Export => {}
            Handler::Quit => self.quit = true,
        }
    }

    /// `/skills`: modal na świeżo odkrytej liście (nowe pliki widać od razu).
    fn open_skills(&mut self) {
        self.refresh_skills();
        self.skills_sel = Some(0);
        self.hints_hidden = true;
        self.toast = None;
    }

    /// `/skill <nazwa>`: treść skilla do kontekstu następnej tury. Nieznana
    /// nazwa i zepsuty nagłówek kończą się komunikatem, nie crashem.
    fn run_skill(&mut self, text: &str) {
        if self.view != View::Session {
            self.open_new_session();
        }
        let name = text.split_whitespace().nth(1).unwrap_or("").to_string();
        if name.is_empty() || name == "list" {
            // Bez nazwy nie ma czego wczytać — pokazujemy listę, jak `/skills`.
            self.open_skills();
            return;
        }
        self.refresh_skills();
        let Some(skill) = crate::skills::find(&self.skills, &name).cloned() else {
            self.toast = Some(format!("nie znam skilla '{}'", name));
            self.push(Entry::new(
                Kind::Info,
                "",
                format!("nie znam skilla '{}' \u{2014} wpisz /skills", name),
            ));
            self.scroll = 0;
            return;
        };
        if let Some(why) = skill.header_error.clone() {
            self.toast = Some(format!("skill '{}' ma blad naglowka", name));
            self.push(Entry::new(
                Kind::Info,
                "",
                format!(
                    "skill '{}' ma blad naglowka ({}) \u{2014} nie wczytuje tresci",
                    name, why
                ),
            ));
            self.scroll = 0;
            return;
        }
        let (content, truncated) = skill.clipped();
        if !self.loaded_skills.iter().any(|n| n == &skill.name) {
            self.loaded_skills.push(skill.name.clone());
        }
        let warning = if truncated {
            format!(
                ", obcieta do {} bajtow",
                crate::skills::MAX_CONTENT_BYTES
            )
        } else {
            String::new()
        };
        self.toast = Some(format!("skill '{}' w kontekscie", skill.name));
        self.push(Entry::new(
            Kind::Info,
            "",
            format!(
                "skill '{}' w kontekscie ({} bajtow{}) \u{2014} tresc pojdzie do modelu w nastepnym zadaniu",
                skill.name,
                content.len(),
                warning
            ),
        ));
        self.scroll = 0;
    }

    /// `Enter` w modalu `/skills`: podgląd treści w scrollbacku (modal się
    /// zamyka, inaczej podgląd zostałby pod nim ukryty).
    fn preview_skill(&mut self, index: usize) {
        let Some(skill) = self.skills.get(index).cloned() else {
            return;
        };
        self.skills_sel = None;
        self.toast = None;
        let (content, _) = skill.clipped();
        let mut text = String::new();
        if let Some(why) = &skill.header_error {
            text.push_str(&format!("blad naglowka: {}\n\n", why));
        }
        text.push_str(&format!(
            "skill '{}' \u{2014} {} \u{00B7} {}\n\n{}",
            skill.name,
            skill.size_label(),
            skill.source.label(),
            content
        ));
        self.push(Entry::new(Kind::Info, "", text));
        self.scroll = 0;
    }

    /// `/settings` bez argumentów: modal na sześciu kluczach z `.agent/config`
    /// (+ wiersze „z env", których nie da się zmienić).
    fn open_settings(&mut self) {
        self.settings_sel = Some(0);
        self.hints_hidden = true;
        self.toast = None;
    }

    /// `←`/`→`/`Enter` w modalu: następna/poprzednia wartość, od razu
    /// zastosowana (motyw, mysz, tryb zgód) i zapisana w `.agent/config`.
    fn change_setting(&mut self, dir: i32) {
        let Some(selected) = self.settings_sel else {
            return;
        };
        // Wiersze „z env" są za sześcioma kluczami — nieedytowalne.
        if selected >= settings::KEYS.len() {
            self.toast = Some("ta wartosc pochodzi ze srodowiska".to_string());
            return;
        }
        self.settings.cycle(selected, dir);
        self.memory = self.settings.memory.clone();
        self.mode = self.settings.approval.as_str().to_string();
        self.apply_mouse();
        if let Err(e) = self.settings.save(&self.workspace) {
            self.toast = Some(e);
            return;
        }
        self.toast = Some(format!(
            "{}: {}",
            settings::Settings::label(selected),
            self.settings.value(selected)
        ));
    }

    /// `/theme jasny` (i `/theme groknight`) — motyw od razu, bez modala.
    fn set_theme(&mut self, theme: Theme) {
        if self.settings.theme == theme {
            return;
        }
        self.settings.theme = theme;
        if let Err(e) = self.settings.save(&self.workspace) {
            self.toast = Some(e);
            return;
        }
        self.toast = Some(format!("motyw: {}", theme.as_str()));
    }

    /// `/theme <nazwa>`: poprawna nazwa zmienia motyw, brak argumentu otwiera
    /// modal (tam widać oba warianty), literówka kończy się podpowiedzią.
    fn run_theme(&mut self, text: &str) {
        let arg = text.split_whitespace().nth(1).unwrap_or("");
        if arg.is_empty() {
            self.run_command(Handler::Settings);
            return;
        }
        match Theme::parse(arg) {
            Some(theme) => self.set_theme(theme),
            None => {
                if self.view != View::Session {
                    self.open_new_session();
                }
                self.push(Entry::new(
                    Kind::Info,
                    "",
                    format!(
                        "motyw '{}' nieznany \u{2014} dozwolone: groknight, jasny",
                        arg
                    ),
                ));
            }
        }
    }


    // ── Klawisze ────────────────────────────────────────────────────────────

    // ── Runda 17: szukanie w scrollbacku ────────────────────────────────────

    /// `Ctrl+F`: modal szukania; powtórne `Ctrl+F` zamyka modal bez skoku,
    /// a `Esc` (w modalu albo w scrollbacku) kończy szukanie i wraca.
    fn open_search(&mut self) {
        if let Some(search) = &mut self.search {
            if search.visible {
                search.visible = false;
                search.editing = false;
            } else {
                search.visible = true;
                search.editing = true;
            }
            return;
        }
        self.search = Some(Search {
            query: String::new(),
            current: 0,
            saved_scroll: self.scroll,
            editing: true,
            visible: true,
        });
        self.hints_hidden = true;
        self.toast = None;
    }

    /// Zamyka szukanie i wraca na przewinięcie sprzed `Ctrl+F`.
    fn close_search(&mut self) {
        let saved = self.search.as_ref().map(|search| search.saved_scroll).unwrap_or(0);
        self.search = None;
        self.scroll = saved;
    }

    /// Wiersze scrollbacku w szerokości, po której liczone są trafienia.
    fn search_rows(&self) -> Vec<Row> {
        self.rows(self.search_width.get().max(8))
    }

    /// Indeksy wierszy z frazą (małe litery, bez rozróżniania wielkości).
    /// Pusto dla pustego zapytania — to nie jest błąd, tylko brak listy.
    fn search_hits(&self) -> Vec<usize> {
        let needle = match &self.search {
            Some(search) => search.query.to_lowercase(),
            None => return Vec::new(),
        };
        if needle.trim().is_empty() {
            return Vec::new();
        }
        self.search_rows()
            .iter()
            .enumerate()
            .filter(|(_, row)| row_text(row).to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    /// Nowe zapytanie zaczyna od pierwszego trafienia.
    fn search_recompute(&mut self) {
        if let Some(search) = &mut self.search {
            search.current = 0;
        }
        self.search_jump();
    }

    /// Ustawia przewinięcie tak, żeby zaznaczone trafienie było w kadrze
    /// (jedna trzecia wysokości od góry).
    fn search_jump(&mut self) {
        let Some(search) = &self.search else { return };
        let hits = self.search_hits();
        let Some(&hit) = hits.get(search.current) else {
            return;
        };
        let total = self.search_rows().len();
        let view = self.search_view.get().max(1);
        let max_start = total.saturating_sub(view);
        let target = hit.saturating_sub(view / 3).min(max_start);
        self.scroll = max_start - target;
    }

    /// `n`/`N` (albo `Ctrl+N`/`Ctrl+P`): następne/poprzednie trafienie.
    fn search_step(&mut self, forward: bool) {
        let len = self.search_hits().len();
        if len == 0 {
            return;
        }
        if let Some(search) = &mut self.search {
            search.current = if forward {
                (search.current + 1) % len
            } else {
                (search.current + len - 1) % len
            };
        }
        self.search_jump();
    }

    fn on_key_search(&mut self, key: KeyEvent, ctrl: bool) {
        if self.search.is_none() {
            return;
        }
        if ctrl {
            match key.code {
                KeyCode::Char('n') => {
                    if let Some(search) = &mut self.search {
                        search.editing = false;
                    }
                    self.search_step(true);
                    return;
                }
                KeyCode::Char('p') => {
                    if let Some(search) = &mut self.search {
                        search.editing = false;
                    }
                    self.search_step(false);
                    return;
                }
                KeyCode::Char('f') => {
                    if let Some(search) = &mut self.search {
                        search.visible = false;
                        search.editing = false;
                    }
                    return;
                }
                _ => {}
            }
        }
        let editing = self.search.as_ref().map(|s| s.editing).unwrap_or(false);
        match key.code {
            KeyCode::Esc => {
                // `Esc` zamyka szukanie i wraca tam, skąd przyszło.
                self.close_search();
            }
            KeyCode::Enter => {
                if let Some(search) = &mut self.search {
                    search.editing = false;
                    // Modal znika: jump ma być widać w scrollbacku, a licznik
                    // i `n`/`N` przejmuje stopka.
                    search.visible = false;
                }
                self.search_jump();
            }
            KeyCode::Backspace if editing => {
                if let Some(search) = &mut self.search {
                    search.query.pop();
                }
                self.search_recompute();
            }
            KeyCode::Up => self.search_step(false),
            KeyCode::Down => self.search_step(true),
            KeyCode::Char('n') if !editing && !ctrl => self.search_step(true),
            KeyCode::Char('N') if !editing && !ctrl => self.search_step(false),
            KeyCode::Char(c)
                if editing && !ctrl && !key.modifiers.contains(KeyModifiers::ALT) && !c.is_control() =>
            {
                if let Some(search) = &mut self.search {
                    search.query.push(c);
                }
                self.search_recompute();
            }
            _ => {}
        }
    }

    /// Modal szukania: trafienia jako `numer wiersza + tekst` z bieżącego
    /// scrollbacku (te same wiersze, które widać po zamknięciu).
    fn draw_search(&self, buf: &mut Buffer, area: Rect, search: &Search) {
        let rows = self.search_rows();
        let hits: Vec<welcome::SearchHit> = self
            .search_hits()
            .iter()
            .filter_map(|&index| {
                rows.get(index).map(|row| welcome::SearchHit {
                    line: index + 1,
                    text: row_text(row).trim().to_string(),
                })
            })
            .collect();
        let state = welcome::SearchState {
            path: welcome::short_path(&self.workspace),
            query: search.query.clone(),
            hits,
            selected: search.current,
            editing: search.editing,
            note: None,
        };
        draw_rows(buf, area, &welcome::search(&state, area.width, area.height));
    }

    /// Stopka w trybie chodzenia po trafieniach (`n`/`N` po `Enter`):
    /// zapytanie, licznik i klawisze zamiast zwykłych podpowiedzi.
    fn draw_search_bar(&self, buf: &mut Buffer, area: Rect, search: &Search) {
        if area.height == 0 || area.width < 10 {
            return;
        }
        for x in area.x..area.x + area.width {
            set_cell(buf, x, area.y, ' ', Style::default().bg(BG));
        }
        let hits = self.search_hits().len();
        let counter = if hits == 0 {
            "0/0".to_string()
        } else {
            format!("{}/{}", search.current + 1, hits)
        };
        let left = format!(
            "szukanie: {}   {}   n/N: nastepne   Esc: koniec",
            search.query, counter
        );
        let right = if hits == 0 { "brak trafien" } else { "" };
        let right_w = right.chars().count() as u16;
        let room = area.width.saturating_sub(right_w + 2);
        put_text(
            buf,
            area.x,
            area.y,
            &left,
            Style::default().fg(GRAY).bg(BG),
            room,
        );
        if !right.is_empty() {
            put_text(
                buf,
                area.x + area.width - right_w,
                area.y,
                right,
                Style::default().fg(YELLOW).bg(BG),
                right_w,
            );
        }
    }

    // ── Runda 17: /copy i /export ───────────────────────────────────────────

    /// Ostatni blok odpowiedzi asystenta: treść `finish` z podsumowania
    /// (albo sam status, gdy model nic nie dopisał).
    fn last_answer(&self) -> Option<String> {
        self.entries
            .iter()
            .rev()
            .find(|entry| entry.kind == Kind::Summary)
            .map(|entry| {
                let body = entry.body.clone().unwrap_or_default();
                if body.trim().is_empty() {
                    entry.subject.clone()
                } else {
                    body
                }
            })
            .filter(|text| !text.trim().is_empty())
    }

    /// Cała widoczna rozmowa w formacie `ty: treść` / `asystent: treść`.
    fn transcript_text(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for entry in &self.entries {
            let line = match entry.kind {
                Kind::Task => format!("ty: {}", entry.subject),
                Kind::Thought => format!("asystent: {}", entry_text(entry)),
                Kind::Call => format!("asystent: {} {}", entry.tool, entry.subject),
                Kind::Ok | Kind::Cached => {
                    format!("wynik: {} {}", entry.tool, entry.subject)
                }
                Kind::Fail | Kind::Skipped => {
                    format!("wynik: {} {} (blad)", entry.tool, entry.subject)
                }
                Kind::Denied => format!("odmowa: {}", entry.subject),
                Kind::Summary => format!(
                    "asystent: {} {}",
                    entry.subject,
                    entry.body.clone().unwrap_or_default()
                )
                .trim()
                .to_string(),
                Kind::Info => format!("system: {}", entry.subject),
                Kind::Error => format!("blad: {}", entry.subject),
            };
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
        lines.join("\n")
    }

    /// `/copy` i `/copy all`: tekst idzie do schowka przez OSC 52.
    fn copy_block(&mut self, all: bool) {
        if self.view != View::Session {
            self.open_new_session();
        }
        let text = if all {
            Some(self.transcript_text())
        } else {
            self.last_answer()
        };
        let Some(text) = text.filter(|text| !text.trim().is_empty()) else {
            self.push(Entry::new(
                Kind::Info,
                "",
                "brak odpowiedzi asystenta do skopiowania",
            ));
            self.scroll = 0;
            self.toast = None;
            return;
        };
        let chars = text.chars().count();
        let message = match crate::clipboard::copy(&text, self.interactive) {
            Ok(()) => format!("skopiowano {} znakow (OSC 52)", chars),
            Err(_) => "schowek moze nie zadzialac w tym terminalu".to_string(),
        };
        self.push(Entry::new(Kind::Info, "", message));
        self.scroll = 0;
        self.toast = None;
    }

    /// `/export [plik]`: nagłówek zadania, data, model i bloki `## krok N`.
    fn export_markdown(&self) -> String {
        let task = self
            .entries
            .iter()
            .find(|entry| entry.kind == Kind::Task)
            .map(|entry| entry.subject.clone())
            .or_else(|| self.session_title.clone())
            .or_else(|| self.last_task.clone())
            .unwrap_or_else(|| "sesja".to_string());
        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", task.trim()));
        out.push_str(&format!("Data: {}\n", crate::clock::date_time()));
        out.push_str(&format!("Model: {}\n", self.model));

        let mut steps: Vec<u64> = self
            .entries
            .iter()
            .filter(|entry| entry.step > 0)
            .map(|entry| entry.step)
            .collect();
        steps.sort_unstable();
        steps.dedup();
        for step in steps {
            out.push_str(&format!("\n## krok {}\n\n", step));
            for entry in self.entries.iter().filter(|entry| entry.step == step) {
                match entry.kind {
                    Kind::Thought => {
                        let text = entry_text(entry);
                        if !text.trim().is_empty() {
                            out.push_str(&format!("mysl: {}\n", clip_chars(&text, 400)));
                        }
                    }
                    Kind::Call => {
                        out.push_str(&format!(
                            "wywolanie: {} {}\n",
                            entry.tool, entry.subject
                        ));
                    }
                    Kind::Ok | Kind::Cached => {
                        out.push_str(&format!(
                            "wynik: {}\nstatus: ok\n",
                            clip_chars(&result_line(entry), 400)
                        ));
                    }
                    Kind::Fail | Kind::Skipped => {
                        out.push_str(&format!(
                            "wynik: {}\nstatus: blad\n",
                            clip_chars(&result_line(entry), 400)
                        ));
                    }
                    Kind::Denied => {
                        out.push_str(&format!(
                            "wynik: {}\nstatus: odmowa\n",
                            clip_chars(&entry.subject, 400)
                        ));
                    }
                    _ => {}
                }
            }
        }

        out.push_str("\n## status koncowy\n\n");
        let status = self.status.clone().unwrap_or_else(|| "w toku".to_string());
        out.push_str(&format!(
            "{} \u{00B7} kroki {} \u{00B7} narzedzia {} \u{00B7} odmowy {} \u{00B7} dryf {}\n",
            status, self.steps, self.tool_calls, self.denied, self.drift
        ));
        out
    }

    /// Zapis eksportu do workspace — te same bramki ścieżki co `fs.write`
    /// (`tools::resolve`): bez `..`, bez ścieżek absolutnych, bez `.agent`.
    fn export(&mut self, arg: Option<&str>) {
        if self.view != View::Session {
            self.open_new_session();
        }
        let name = arg
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| format!("eksport-{}.md", crate::clock::stamp_compact()));
        let step_count = self
            .entries
            .iter()
            .filter(|entry| entry.step > 0)
            .map(|entry| entry.step)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        match tools::resolve(&self.workspace, &name) {
            Ok(path) => {
                let text = self.export_markdown();
                let message = match std::fs::write(&path, text.as_bytes()) {
                    Ok(()) => format!("zapisano eksport: {} ({} krokow)", name, step_count),
                    Err(e) => format!("nie mozna zapisac eksportu {}: {}", name, e),
                };
                self.push(Entry::new(Kind::Info, "", message));
            }
            Err(result) => {
                let reason = result
                    .output
                    .get("message")
                    .and_then(|value| value.as_str())
                    .unwrap_or("odmowa");
                self.push(Entry::new(
                    Kind::Info,
                    "",
                    format!("eksport odrzucony: {} \u{2014} {}", name, reason),
                ));
            }
        }
        self.scroll = 0;
        self.toast = None;
    }

    fn on_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('d')) {
            self.quit = true;
            return;
        }
        match self.view {
            View::Welcome => self.on_key_welcome(key, ctrl),
            View::Resume => self.on_key_resume(key, ctrl),
            View::Sessions => self.on_key_list(key, ctrl),
            View::Session => self.on_key_session(key, ctrl),
        }
    }

    /// `ctrl+r`: modal „Resume session" na świeżo odczytanych transkryptach.
    fn open_resume(&mut self) {
        self.refresh_sessions();
        self.query.clear();
        self.searching = false;
        self.selected = 0;
        self.list_offset = 0;
        self.toast = None;
        self.view = View::Resume;
    }

    /// `ctrl+w`: lista sesji (`views/dashboard`). Ekran zostaje w programie,
    /// tylko przestaje być wejściem do sesji — tym jest modal.
    fn open_list(&mut self) {
        self.refresh_sessions();
        self.input.clear();
        self.cursor = 0;
        self.toast = None;
        self.view = View::Sessions;
    }

    /// Klawisze ekranu powitalnego: menu (`ctrl+r`, `ctrl+w`, `ctrl+q`),
    /// `ctrl+n` nowa sesja, `Enter` wysyła wpisane zadanie.
    fn on_key_welcome(&mut self, key: KeyEvent, ctrl: bool) {
        match key.code {
            KeyCode::Char('r') if ctrl => self.open_resume(),
            KeyCode::Char('w') if ctrl => self.open_list(),
            KeyCode::Char('n') if ctrl => self.open_new_session(),
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Enter => {
                let task = self.input.trim().to_string();
                // `/help`, `/resume`, `/quit` działają też z ekranu powitalnego.
                if task.starts_with('/') {
                    self.input = task;
                    self.cursor = self.input.chars().count();
                    self.dispatch();
                    return;
                }
                self.input.clear();
                self.cursor = 0;
                self.open_new_session();
                if !task.is_empty() {
                    self.input = task;
                    self.cursor = self.input.chars().count();
                    self.submit();
                }
            }
            KeyCode::Esc => self.quit = true,
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let at = byte_index(&self.input, self.cursor - 1);
                    self.input.remove(at);
                    self.cursor -= 1;
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => {
                let len = self.input.chars().count();
                if self.cursor < len {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                let at = byte_index(&self.input, self.cursor);
                self.input.insert(at, c);
                self.cursor += 1;
            }
            _ => {}
        }
    }

    /// Klawisze modala: `/` szukanie w tytule, `f` filtr (wszystkie/błędy/
    /// w toku), `↑`/`↓`, `Enter` otwiera sesję, `Esc` zamyka modal.
    fn on_key_resume(&mut self, key: KeyEvent, ctrl: bool) {
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                if self.selected + 1 < self.cursor_rows().len() {
                    self.selected += 1;
                }
            }
            KeyCode::Enter => self.open_selected_session(),
            KeyCode::Esc => {
                if self.searching || !self.query.is_empty() {
                    // Pierwszy `Esc` wychodzi z szukania, drugi zamyka modal.
                    self.query.clear();
                    self.searching = false;
                    self.selected = 0;
                } else {
                    self.toast = None;
                    self.view = View::Welcome;
                }
            }
            KeyCode::Char('/') if !ctrl && !self.searching => self.searching = true,
            KeyCode::Char('f') if !ctrl && !self.searching => {
                self.filter = self.filter.next();
                self.selected = 0;
                self.clamp_selection();
            }
            KeyCode::Char('w') if ctrl => self.open_list(),
            KeyCode::Char('q') if ctrl => self.quit = true,
            KeyCode::Backspace if self.searching => {
                self.query.pop();
                self.selected = 0;
                self.clamp_selection();
            }
            KeyCode::Char(c) if !ctrl && self.searching && !c.is_control() => {
                self.query.push(c);
                self.selected = 0;
                self.clamp_selection();
            }
            _ => {}
        }
    }

    /// Klawisze dashboardu: `↑`/`↓`, `Enter`, `n`, `/`, `Esc`, plus pisanie
    /// w dolnym polu (nowe zadanie bez wchodzenia w sesję).
    fn on_key_list(&mut self, key: KeyEvent, ctrl: bool) {
        // Skróty (strzałki, `n`, `/`) działają tylko przy pustym polu: inaczej
        // wpisanie zadania z literą `n` otwierałoby nową sesję zamiast ją pisać.
        let typing = !self.input.is_empty();
        match key.code {
            KeyCode::Up if !typing => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down if !typing => {
                if self.selected + 1 < self.visible_sessions().len() {
                    self.selected += 1;
                }
            }
            KeyCode::Enter => {
                let task = self.input.trim().to_string();
                if !task.is_empty() {
                    // Pole na dole dashboardu: nowa sesja z tym zadaniem.
                    self.input.clear();
                    self.cursor = 0;
                    self.open_new_session();
                    self.submit();
                } else {
                    self.open_selected_session();
                }
            }
            KeyCode::Char('n') if !ctrl && !typing => self.open_new_session(),
            KeyCode::Char('/') if !ctrl && !typing => {
                self.filter = self.filter.next();
                self.selected = 0;
                self.list_offset = 0;
                self.clamp_selection();
            }
            KeyCode::Esc => self.quit = true,
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let at = byte_index(&self.input, self.cursor - 1);
                    self.input.remove(at);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.input.chars().count() {
                    let at = byte_index(&self.input, self.cursor);
                    self.input.remove(at);
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => {
                let len = self.input.chars().count();
                if self.cursor < len {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                let at = byte_index(&self.input, self.cursor);
                self.input.insert(at, c);
                self.cursor += 1;
            }
            _ => {}
        }
    }

    fn on_key_session(&mut self, key: KeyEvent, ctrl: bool) {
        // Tryb `ask`: pytanie w composerze przechwytuje klawisze, dopóki nie
        // padnie odpowiedź. `t`/Enter zgoda, `n`/Esc odmowa.
        if self.question.is_some() {
            match key.code {
                KeyCode::Char('t') | KeyCode::Char('T') | KeyCode::Char('y') | KeyCode::Enter => {
                    self.answer_question(true)
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.answer_question(false),
                _ => {}
            }
            return;
        }
        // Runda 17: modal szukania przechwytuje klawisze, dopóki się nie zamknie.
        let search_visible = self
            .search
            .as_ref()
            .map(|search| search.visible)
            .unwrap_or(false);
        if search_visible {
            self.on_key_search(key, ctrl);
            return;
        }
        // Po `Enter` modal znika, ale szukanie trwa: `n`/`N` chodzą po
        // trafieniach, `Esc` kończy i wraca, `Ctrl+F` otwiera modal z powrotem.
        if self.search.is_some() {
            match key.code {
                KeyCode::Esc => {
                    self.close_search();
                    return;
                }
                KeyCode::Char('f') if ctrl => {
                    self.open_search();
                    return;
                }
                // Litery wpisywane w composerze mają pierwszeństwo: `n` chodzi
                // po trafieniach tylko przy pustym polu (inaczej nie dałoby się
                // napisać „notatka").
                KeyCode::Char('n') if !ctrl && self.input.is_empty() => {
                    self.search_step(true);
                    return;
                }
                KeyCode::Char('N') if !ctrl && self.input.is_empty() => {
                    self.search_step(false);
                    return;
                }
                _ => {}
            }
        }
        // Modal `/skills` (runda 16): `↑/↓` wybór, `Enter` podgląd treści
        // w scrollbacku, `Esc` zamknięcie.
        if let Some(selected) = self.skills_sel {
            match key.code {
                KeyCode::Up => self.skills_sel = Some(selected.saturating_sub(1)),
                KeyCode::Down => {
                    if selected + 1 < self.skills.len() {
                        self.skills_sel = Some(selected + 1);
                    }
                }
                KeyCode::Enter => self.preview_skill(selected),
                KeyCode::Esc => {
                    self.skills_sel = None;
                    self.toast = None;
                }
                _ => {}
            }
            return;
        }
        // Modal `/settings` przechwytuje klawisze: `↑/↓` wybór, `Enter`/`←→`
        // zmiana wartości, `Esc` zamknięcie.
        if let Some(selected) = self.settings_sel {
            let rows = settings::KEYS.len() + settings::env_rows().len();
            match key.code {
                KeyCode::Up => self.settings_sel = Some(selected.saturating_sub(1)),
                KeyCode::Down => {
                    if selected + 1 < rows {
                        self.settings_sel = Some(selected + 1);
                    }
                }
                KeyCode::Left => self.change_setting(-1),
                KeyCode::Right | KeyCode::Enter => self.change_setting(1),
                KeyCode::Esc => {
                    self.settings_sel = None;
                    self.toast = None;
                }
                _ => {}
            }
            return;
        }
        // Modal `/model` przechwytuje klawisze, dopóki się nie zamknie.
        if let Some(selected) = self.picker {
            match key.code {
                KeyCode::Up => self.picker = Some(selected.saturating_sub(1)),
                KeyCode::Down => {
                    if selected + 1 < self.catalog.models.len() {
                        self.picker = Some(selected + 1);
                    }
                }
                KeyCode::Enter => self.select_model(selected),
                KeyCode::Esc => {
                    self.picker = None;
                    self.toast = None;
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('l') if ctrl => {
                self.entries.clear();
                self.scroll = 0;
            }
            // Runda 17: `Ctrl+F` — szukanie w scrollbacku.
            KeyCode::Char('f') if ctrl => self.open_search(),
            KeyCode::Char('u') if ctrl => {
                self.input.clear();
                self.cursor = 0;
                self.hist_pos = None;
            }
            // `Enter` przy podpowiedziach uzupełnia komendę (drugi `Enter` ją
            // uruchamia), bez podpowiedzi wysyła zadanie do pętli. Dokładnie
            // wpisana komenda idzie od razu do rejestru.
            KeyCode::Enter => {
                let exact = commands::exact(self.input.trim()).is_some();
                if !exact && self.complete_hint() {
                    return;
                }
                self.submit();
            }
            KeyCode::Esc => {
                if !self.hints().is_empty() {
                    // Pierwszy `Esc` chowa podpowiedzi, drugi wraca do listy.
                    self.hints_hidden = true;
                } else {
                    self.back_to_list();
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    let at = byte_index(&self.input, self.cursor - 1);
                    self.input.remove(at);
                    self.cursor -= 1;
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.input.chars().count() {
                    let at = byte_index(&self.input, self.cursor);
                    self.input.remove(at);
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => {
                let len = self.input.chars().count();
                if self.cursor < len {
                    self.cursor += 1;
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Tab => {
                self.complete_hint();
            }
            KeyCode::Up => {
                if self.hints().is_empty() {
                    self.history_prev();
                } else {
                    self.hints_sel = self.hints_sel.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if self.hints().is_empty() {
                    self.history_next();
                } else {
                    let len = self.hints().len();
                    if self.hints_sel + 1 < len {
                        self.hints_sel += 1;
                    }
                }
            }
            KeyCode::PageUp => self.scroll = self.scroll.saturating_add(5),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_sub(5),
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                let at = byte_index(&self.input, self.cursor);
                self.input.insert(at, c);
                self.cursor += 1;
                self.hist_pos = None;
                // Pisanie po `Esc` znowu pokazuje podpowiedzi `/`.
                self.hints_hidden = false;
                self.hints_sel = 0;
            }
            _ => {}
        }
    }

    /// Wejście w wybraną sesję: transkrypt z dysku wraca do scrollbacku.
    fn open_selected_session(&mut self) {
        let id = {
            let visible = self.cursor_rows();
            match visible.get(self.selected) {
                Some(row) => row.id.clone(),
                None => return,
            }
        };
        let events = sessions::transcript(&self.workspace, &id);
        if events.is_empty() {
            self.toast = Some(format!("sesja {} jest pusta", short(&id, 8)));
            return;
        }
        self.entries.clear();
        self.status = None;
        self.steps = 0;
        self.tool_calls = 0;
        self.denied = 0;
        self.drift = 0;
        self.scroll = 0;
        self.input.clear();
        self.cursor = 0;
        // Nowa sesja = nowy licznik: `usage` poprzedniej nie może zostać.
        self.context_chars = 0;
        self.context_tokens = None;
        crate::provider::reset_usage();
        for event in events {
            self.handle_event(event);
        }
        self.running = false;
        self.turn_started = None;
        self.session = Some(id);
        self.session_title = self
            .entries
            .iter()
            .find(|e| e.kind == Kind::Task)
            .map(|e| e.subject.clone());
        self.view = View::Session;
    }

    /// `n`: pusta sesja, gotowa na nowe zadanie (bez wchodzenia w historię).
    pub fn open_new_session(&mut self) {
        self.entries.clear();
        self.input.clear();
        self.cursor = 0;
        self.scroll = 0;
        self.status = None;
        self.steps = 0;
        self.tool_calls = 0;
        self.denied = 0;
        self.drift = 0;
        self.session = None;
        self.session_title = None;
        self.toast = None;
        self.context_chars = 0;
        self.context_tokens = None;
        crate::provider::reset_usage();
        self.view = View::Session;
        // Config czytany na starcie: ostrzeżenia wracają w każdej nowej sesji.
        self.push_startup_warnings();
    }

    /// Powrót do listy: stan sesji zostaje w transkrypcie, nie w pamięci ekranu.
    pub fn back_to_list(&mut self) {
        self.view = View::Sessions;
        self.input.clear();
        self.cursor = 0;
        self.refresh_sessions();
    }

    fn on_mouse(&mut self, kind: MouseEventKind) {
        match kind {
            MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_add(3),
            MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_sub(3),
            _ => {}
        }
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.hist_pos {
            Some(p) => p.saturating_sub(1),
            None => self.history.len() - 1,
        };
        self.hist_pos = Some(pos);
        self.input = self.history[pos].clone();
        self.cursor = self.input.chars().count();
    }

    fn history_next(&mut self) {
        match self.hist_pos {
            Some(p) if p + 1 < self.history.len() => {
                self.hist_pos = Some(p + 1);
                self.input = self.history[p + 1].clone();
                self.cursor = self.input.chars().count();
            }
            Some(_) => {
                self.hist_pos = None;
                self.input.clear();
                self.cursor = 0;
            }
            None => {}
        }
    }

    fn submit(&mut self) {
        if self.running {
            self.toast = Some("trwa zadanie — poczekaj na wynik".to_string());
            return;
        }
        let task = self.input.trim().to_string();
        if task.is_empty() {
            return;
        }
        // Komendy `/` obsługuje rejestr — nie idą do modelu.
        if task.starts_with('/') {
            self.dispatch();
            return;
        }
        self.input.clear();
        self.cursor = 0;
        self.history.push(task.clone());
        self.hist_pos = None;
        self.scroll = 0;
        self.status = None;
        self.steps = 0;
        self.tool_calls = 0;
        self.denied = 0;
        self.drift = 0;
        if let Some(id) = self.session.clone() {
            // Wznowienie musi trafic w tę samą sesję: wskaźnik na dysku to
            // jedyny sposób, w jaki `agent::run` ją odnajduje.
            crate::write_resume_pointer(&self.workspace, &id);
        }
        self.session_title = Some(task.clone());
        self.last_task = Some(task.clone());
        self.toast = None;
        // Runda 16: lista skilli czytana przed każdą turą — nowy plik na dysku
        // wchodzi do kontekstu bez restartu programu.
        self.refresh_skills();
        self.running = true;
        self.turn_started = Some(Instant::now());
        self.turn_first_thought = true;
        self.turn_chars = 0;

        let (event_tx, event_rx) = mpsc::channel();
        let (delta_tx, delta_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        self.events = Some(event_rx);
        self.deltas = Some(delta_rx);
        self.rx = Some(done_rx);
        // Sink podpięty PRZED startem pętli, żeby nie zgubić eventu `task`.
        context::set_event_sink(Some(event_tx));
        provider::set_delta_sink(Some(delta_tx));
        let mut cfg = self.config();
        // Runda 15: tryb `ask` dostaje bramkę zgód; w `always-approve` nic
        // się nie zmienia (żadnego pytania, żadnych kanałów).
        self.question = None;
        self.approval = None;
        if self.settings.approval == Approval::Ask {
            let (approver, handle) = agent::Approver::channel();
            cfg.approver = Some(approver);
            self.approval = Some(handle);
        }
        std::thread::spawn(move || {
            let report = agent::run(&task, &cfg);
            let _ = done_tx.send(Msg::Done(report));
        });
    }

    /// `t`/`n` w composerze: odpowiedź na pytanie trybu `ask`. Wątek pętli
    /// czeka na bramce, więc odpowiedź od razu go odblokowuje.
    fn answer_question(&mut self, allow: bool) {
        let Some(question) = self.question.take() else {
            return;
        };
        if let Some(handle) = &self.approval {
            handle.answer(allow);
        }
        self.push(Entry::new(
            Kind::Info,
            "",
            format!(
                "{}: {} {}",
                if allow { "zgoda" } else { "odmowa" },
                question.tool,
                question.arg
            ),
        ));
    }

    /// Przechwytywanie myszy wg ustawienia (`on`/`off`). Bez terminala (zrzuty,
    /// testy) wywołanie nic nie robi — `execute!` na buforze nie istnieje.
    fn apply_mouse(&self) {
        if !self.interactive {
            return;
        }
        let _ = if self.settings.mouse {
            crossterm::execute!(std::io::stdout(), EnableMouseCapture)
        } else {
            crossterm::execute!(std::io::stdout(), DisableMouseCapture)
        };
    }

    // ── Rysowanie ───────────────────────────────────────────────────────────

    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let buf = f.buffer_mut();
        buf.set_style(area, Style::default().bg(BG));
        match self.view {
            View::Welcome => self.draw_welcome(buf, area),
            View::Resume => self.draw_resume(buf, area),
            View::Sessions => {
                self.draw_list(buf, area);
            }
            View::Session => {
                let mut screen = layout(area, self.running, self.composer_rows(area));
                // Podpowiedzi `/` dostają własne wiersze nad composerem —
                // scrollback się kurczy, a nie znika pod panelem.
                if self.picker.is_none()
                    && self.settings_sel.is_none()
                    && self.skills_sel.is_none()
                    && !self
                        .search
                        .as_ref()
                        .map(|search| search.visible)
                        .unwrap_or(false)
                {
                    screen.reserve_hints(self.hints().len() as u16);
                }
                self.draw_top(buf, screen.top);
                self.draw_scrollback(buf, screen.scrollback);
                if screen.turn.height > 0 {
                    self.draw_stream(buf, screen.turn);
                }
                self.draw_prompt(buf, screen.prompt);
                self.draw_footer(buf, screen.footer);
                // Modal `/model` i `/settings` zakrywają cały ekran (jak
                // „Resume session"); bez nich podpowiedzi `/` leżą tuż nad
                // composerem.
                match (self.picker, self.settings_sel, self.skills_sel) {
                    (_, _, Some(selected)) => self.draw_skills(buf, area, selected),
                    (Some(selected), _, None) => self.draw_picker(buf, area, selected),
                    (None, Some(selected), None) => self.draw_settings(buf, area, selected),
                    (None, None, None) => self.draw_hints(buf, screen.hints),
                }
                // Runda 17: modal szukania jest nad wszystkim innym; po
                // `Enter` znika, a licznik trafień staje w stopce.
                if let Some(search) = &self.search {
                    if search.visible {
                        self.draw_search(buf, area, search);
                    } else {
                        self.draw_search_bar(buf, screen.footer, search);
                    }
                }
            }
        }
        // Motyw `jasny`: jeden przebieg podmiany kolorów po narysowaniu
        // wszystkiego — układ i styl zostają bez zmian (patrz `theme.rs`).
        if self.settings.theme == Theme::Light {
            crate::theme::recolor(buf);
        }
    }

    /// Ekran powitalny — wiersze z [`welcome::welcome`], geometria ze zrzutu
    /// `welcome.txt`: ścieżka, logo, menu, ogłoszenia, composer, wersja.
    fn draw_welcome(&self, buf: &mut Buffer, area: Rect) {
        let state = welcome::WelcomeState {
            path: welcome::short_path(&self.workspace),
            model: self.model.clone(),
            mode: self.mode.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            announcements: vec![
                "ArndtAi TUI jest gotowe.".to_string(),
                "Wybierz 'Resume session' albo wpisz zadanie.".to_string(),
            ],
            tip: "Tip: Ctrl+R wznawia sesje, Ctrl+W otwiera liste.".to_string(),
            note: self.toast.clone(),
        };
        draw_rows(buf, area, &welcome::welcome(&state, area.width, area.height));
    }

    /// Modal „Resume session" — lista prawdziwych transkryptów pod filtrem
    /// i szukaniem; zaznaczenie to pasek `bg_light`.
    fn draw_resume(&self, buf: &mut Buffer, area: Rect) {
        let own: Vec<SessionRow> = self.resume_sessions().into_iter().cloned().collect();
        let state = welcome::ResumeState {
            path: welcome::short_path(&self.workspace),
            sessions: &own,
            selected: self.selected,
            offset: 0,
            query: self.query.clone(),
            searching: self.searching,
            filter: self.filter,
            now: sessions::now_secs(),
            note: self.toast.clone(),
        };
        draw_rows(buf, area, &welcome::resume(&state, area.width, area.height));
    }

    /// Szerokość tekstu w composerze przy danym ekranie — ta sama, którą
    /// liczy `draw_prompt` (ramka 1 kolumna + `❯ ` + ramka).
    fn composer_text_width(area: Rect) -> u16 {
        let hpad = if area.height <= AUTO_COMPACT_MAX_ROWS {
            COMPACT_HPAD
        } else {
            OUTER_HPAD
        };
        let inner = area.width.saturating_sub(hpad.saturating_mul(2));
        inner.saturating_sub(4).max(4)
    }

    fn composer_rows(&self, area: Rect) -> u16 {
        composer_grid(&self.input, Self::composer_text_width(area) as usize)
            .len()
            .max(1) as u16
    }

    /// Górny pasek ze zrzutu `readme-tui.png`: skrócona ścieżka po lewej,
    /// licznik kontekstu po prawej, oba przygaszone, bez środka.
    fn draw_top(&self, buf: &mut Buffer, area: Rect) {
        let style = Style::default().fg(GRAY).bg(BG);
        buf.set_style(area, Style::default().bg(BG));
        if area.width < 10 {
            return;
        }
        // Licznik kontekstu: realne tokeny z `usage` albo szacunek z znaków
        // z jawnym `~`; limit to `limit.context` aktywnego modelu (`?`, gdy
        // provider go nie podał). Kolor zmienia się przy 70% i 90% limitu.
        let (used, estimated) = self.context_used();
        let limit = self.catalog.find(&self.model).and_then(|m| m.context);
        let ratio = match limit {
            Some(limit) if limit > 0 => used as f64 / limit as f64,
            _ => 0.0,
        };
        let counter_style = if ratio >= 0.9 {
            Style::default().fg(RED).bg(BG)
        } else if ratio >= 0.7 {
            Style::default().fg(YELLOW).bg(BG)
        } else {
            style
        };
        let counter = format!(
            "{}{} / {}",
            if estimated && used > 0 { "~" } else { "" },
            models::tokens_label(used),
            models::limit_label(limit)
        );
        let counter_w = (counter.chars().count() as u16).min(area.width);
        let counter_x = area.x + area.width.saturating_sub(counter_w);

        // Ścieżka dostaje wszystko, co zostaje po liczniku i dwóch kolumnach oddechu.
        let path = welcome::short_path(&self.workspace);
        let room = counter_x.saturating_sub(area.x).saturating_sub(2);
        let path = trim_left(&path, room as usize);
        put_text(buf, area.x, area.y, &path, style, room);
        put_text(buf, counter_x, area.y, &counter, counter_style, counter_w);
    }

    /// Wiersz aktywności tury: `⠋ Responding… 15s` po lewej, `17s ↓9.45k`
    /// po prawej. Czas jest mierzony od wysłania zadania, licznik po prawej to
    /// znaki zdarzeń tej tury (provider nie zwraca tokenów).
    fn draw_stream(&self, buf: &mut Buffer, area: Rect) {
        buf.set_style(area, Style::default().bg(BG));
        let elapsed = self
            .turn_started
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0);
        let dim = Style::default().fg(GRAY_DIM).bg(BG);
        let mut x = area.x;
        x += put_text(
            buf,
            x,
            area.y,
            &format!("{} ", self.spinner()),
            Style::default().fg(GRAY).bg(BG),
            area.width,
        );
        let label = if elapsed > 0 {
            format!("Responding\u{2026} {}s", elapsed)
        } else {
            "Responding\u{2026}".to_string()
        };
        x += put_text(buf, x, area.y, &label, dim, area.width.saturating_sub(x - area.x));

        let right = if self.turn_chars > 0 {
            format!(
                "{}s \u{2193}{:.2}k",
                elapsed,
                self.turn_chars as f64 / 1000.0
            )
        } else {
            format!("{}s", elapsed)
        };
        let right_w = (right.chars().count() as u16).min(area.width);
        if area.x + area.width >= right_w + 2 {
            put_text(
                buf,
                area.x + area.width - right_w,
                area.y,
                &right,
                dim,
                right_w,
            );
        }
        let _ = x;
    }

    fn draw_scrollback(&self, buf: &mut Buffer, area: Rect) {
        buf.set_style(area, Style::default().bg(BG));
        if area.height == 0 || area.width < 8 {
            return;
        }
        // Treść idzie pełną szerokością (bez kolumny akcentu i bez wcięcia
        // bloku); ostatnia kolumna należy do paska przewijania.
        let content_w = area.width.saturating_sub(SCROLLBAR_WIDTH);
        // Runda 17: trafienia liczone są po tej samej szerokości i wysokości,
        // po których rysuje się scrollback (modal pokazuje te same wiersze).
        self.search_width.set(content_w as usize);
        self.search_view.set(area.height as usize);
        let rows = self.rows(content_w as usize);
        let view = area.height as usize;
        let max_start = rows.len().saturating_sub(view);
        let back = self.scroll.min(max_start);
        let start = max_start - back;
        let needle = self
            .search
            .as_ref()
            .map(|search| search.query.to_lowercase())
            .unwrap_or_default();

        for (i, row) in rows.iter().skip(start).take(view).enumerate() {
            let y = area.y + i as u16;
            if let Some(bg) = row.bg {
                // Pas zadania pokrywa całą szerokość treści, także pod tekstem.
                for x in area.x..area.x + area.width {
                    set_cell(buf, x, y, ' ', Style::default().bg(bg));
                }
            }
            // Trafienia podświetlamy odwróceniem kolorów — paleta zostaje ta sama.
            let spans = if needle.trim().is_empty() {
                row.spans.clone()
            } else {
                highlight_spans(&row.spans, &needle)
            };
            paint_spans(buf, area.x, y, &spans, content_w);
            if let Some((text, style)) = &row.right {
                let len = text.chars().count() as u16;
                if len + 1 < content_w {
                    put_text(
                        buf,
                        area.x + content_w - len,
                        y,
                        text,
                        *style,
                        content_w,
                    );
                }
            }
        }

        // Pasek przewijania: track + thumb, kolumna przy prawej krawędzi.
        if rows.len() > view && area.height > 0 {
            let x = area.x + area.width - SCROLLBAR_WIDTH;
            for y in area.y..area.y + area.height {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(BG_TERMINAL));
                }
            }
            let thumb_h = ((view * view) / rows.len()).max(1) as u16;
            let span = (area.height - thumb_h) as usize;
            let pos = if max_start == 0 {
                0
            } else {
                (start * span / max_start).min(span) as u16
            };
            for y in 0..thumb_h {
                if let Some(cell) = buf.cell_mut((x, area.y + pos + y)) {
                    cell.set_char(' ');
                    cell.set_style(Style::default().bg(BG_LIGHT));
                }
            }
        }
    }

    fn rows(&self, width: usize) -> Vec<Row> {
        let mut rows = Vec::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if index > 0 {
                // `gap_after: 1` z `scrollback/state/layout.rs`: dokładnie
                // jeden pusty wiersz między wpisami, także po bloku
                // wieloliniowym (myśl z treścią).
                rows.push(Row::plain(Vec::new()));
            }
            rows.extend(entry_rows(
            entry,
            width.max(8),
            self.settings.timestamps,
            &self.md,
        ));
        }
        if rows.is_empty() {
            rows.push(Row::plain(vec![Span::styled(
                "Pusty scrollback \u{2014} wpisz zadanie ponizej.",
                Style::default().fg(GRAY),
            )]));
        }
        rows
    }

    fn draw_prompt(&self, buf: &mut Buffer, area: Rect) {
        if area.height == 0 || area.width < 6 {
            return;
        }
        // Tryb `ask`: composer pokazuje pytanie o zgodę zamiast wpisywanego
        // zadania — wątek pętli czeka na `t`/`n`.
        if let Some(question) = &self.question {
            self.draw_question(buf, area, question);
            return;
        }
        let focused = !self.running;
        let border_style = Style::default().fg(PROMPT_BORDER).bg(BG);
        buf.set_style(area, Style::default().bg(BG));

        let left = area.x;
        let right = area.x + area.width - 1;
        let top_y = area.y;
        let bottom_y = area.y + area.height - 1;

        // Górna krawędź `╭──────╮` — bez podpisu, jak w zrzucie.
        for x in left..=right {
            let ch = if x == left {
                '\u{256D}'
            } else if x == right {
                '\u{256E}'
            } else {
                '\u{2500}'
            };
            set_cell(buf, x, top_y, ch, border_style);
        }

        // Wiersze tekstu: `│ ❯ tekst… │`, zawijane twardo co `text_w` znaków,
        // z przewijaniem, żeby kursor zawsze został w ramce.
        let inner_x = left + 1;
        let inner_w = area.width.saturating_sub(2);
        let text_w = inner_w.saturating_sub(2).max(1) as usize;
        let grid = composer_grid(&self.input, text_w);
        let visible = area.height.saturating_sub(2) as usize;
        let (cur_row, cur_col) = {
            let chars = self.input.chars().count();
            (chars / text_w, chars % text_w)
        };
        let offset = cur_row.saturating_sub(visible.saturating_sub(1));
        let fg_style = Style::default().fg(FG).bg(BG);
        let blink_on = (self.tick_count / 10) % 2 == 0;
        let cursor_style = if focused && blink_on {
            Style::default().fg(BG).bg(FG_DARK)
        } else {
            fg_style
        };

        for row in 0..visible {
            let y = area.y + 1 + row as u16;
            set_cell(buf, left, y, '\u{2502}', border_style);
            set_cell(buf, right, y, '\u{2502}', border_style);
            let index = offset + row;
            let prefix = if index == 0 { PROMPT_ARROW } else { "  " };
            put_text(
                buf,
                inner_x,
                y,
                prefix,
                Style::default().fg(if focused { FG_DARK } else { GRAY }).bg(BG),
                2,
            );
            let Some(line) = grid.get(index) else {
                continue;
            };
            let text_x = inner_x + 2;
            put_text(buf, text_x, y, line, fg_style, text_w as u16);
            // Kursor blokowy na swoim znaku (albo na spacji za tekstem).
            if index == cur_row && visible > 0 {
                let cell_ch = line.chars().nth(cur_col).unwrap_or(' ');
                set_cell(buf, text_x + cur_col as u16, y, cell_ch, cursor_style);
            }
        }
        // Placeholder kładziemy po tekście i kursorze (jak pager): przy pustym
        // wejściu ma zostać cały, a kursor tylko miga na jego tle.
        if self.input.is_empty() && visible > 0 {
            put_text(
                buf,
                inner_x + 2,
                area.y + 1,
                "Zadanie dla ArndtAi…",
                Style::default().fg(GRAY).bg(BG),
                text_w as u16,
            );
        }

        // Dolna krawędź `╰──── model · tryb ────╯` — etykieta wpisana w linię,
        // po prawej, jak w zrzucie (`Grok 4.5 (high) · always-approve`).
        for x in left..=right {
            let ch = if x == left {
                '\u{2570}'
            } else if x == right {
                '\u{256F}'
            } else {
                '\u{2500}'
            };
            set_cell(buf, x, bottom_y, ch, border_style);
        }
        let info = format!(" {} · {} ", self.model, self.mode);
        let info_w = info.chars().count() as u16;
        if area.width > info_w + 3 {
            put_text(
                buf,
                right - info_w - 1,
                bottom_y,
                &info,
                Style::default().fg(GRAY_DIM).bg(BG),
                info_w,
            );
        }
    }

    /// Modal `/model`: pełna ramka w stylu „Resume session" — tytuł, lista
    /// (nazwa, id, okno kontekstu, cena, znacznik aktywnego), stopka klawiszy.
    fn draw_picker(&self, buf: &mut Buffer, area: Rect, selected: usize) {
        let rows: Vec<welcome::PickerRow> = self
            .catalog
            .models
            .iter()
            .map(|model| welcome::PickerRow {
                name: model.name.clone(),
                id: model.id.clone(),
                context: models::limit_label(model.context),
                price: models::price_label(model.price_in, model.price_out),
                active: model.id == self.model,
            })
            .collect();
        let state = welcome::PickerState {
            path: welcome::short_path(&self.workspace),
            models: rows,
            selected,
            note: self.toast.clone(),
        };
        draw_rows(buf, area, &welcome::picker(&state, area.width, area.height));
    }

    /// Modal `/skills` (runda 16): nazwa, opis, rozmiar treści i znacznik
    /// „w kontekście". Rysuje `welcome::skills`, więc układ jest ten sam co
    /// w `/settings` i pickerze modeli.
    fn draw_skills(&self, buf: &mut Buffer, area: Rect, selected: usize) {
        let state = welcome::SkillsState {
            path: welcome::short_path(&self.workspace),
            rows: self
                .skills
                .iter()
                .map(|skill| welcome::SkillRow {
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    size: skill.size_label(),
                    broken: skill.broken(),
                    loaded: self.loaded_skills.iter().any(|n| n == &skill.name),
                })
                .collect(),
            selected,
            note: self.toast.clone(),
        };
        draw_rows(buf, area, &welcome::skills(&state, area.width, area.height));
    }

    /// Modal `/settings`: te same ramki co „Select model", wartości po prawej,
    /// wiersze „z env" przygaszone (środowisko ma pierwszeństwo, więc nie da
    /// się ich zmienić).
    fn draw_settings(&self, buf: &mut Buffer, area: Rect, selected: usize) {
        let mut rows: Vec<welcome::SettingRow> = (0..settings::KEYS.len())
            .map(|index| welcome::SettingRow {
                label: Settings::label(index).to_string(),
                value: self.settings.value(index),
                editable: true,
                from_env: false,
            })
            .collect();
        for (key, value) in settings::env_rows() {
            rows.push(welcome::SettingRow {
                label: key,
                value,
                editable: false,
                from_env: true,
            });
        }
        let state = welcome::SettingsState {
            path: welcome::short_path(&self.workspace),
            rows,
            selected,
            note: self.toast.clone(),
        };
        draw_rows(buf, area, &welcome::settings(&state, area.width, area.height));
    }

    /// Pytanie trybu `ask` w composerze: `Zezwolić na fs.write nota.txt? [t/n]`.
    /// Ramka i etykieta `model · tryb` zostają te same co przy zwykłym wpisie.
    fn draw_question(&self, buf: &mut Buffer, area: Rect, question: &agent::ApprovalRequest) {
        if area.height == 0 || area.width < 6 {
            return;
        }
        let border_style = Style::default().fg(PROMPT_BORDER_ACTIVE).bg(BG);
        buf.set_style(area, Style::default().bg(BG));
        let left = area.x;
        let right = area.x + area.width - 1;
        let top_y = area.y;
        let bottom_y = area.y + area.height - 1;
        for x in left..=right {
            let ch = if x == left {
                '\u{256D}'
            } else if x == right {
                '\u{256E}'
            } else {
                '\u{2500}'
            };
            set_cell(buf, x, top_y, ch, border_style);
        }
        let inner_x = left + 1;
        let text_w = area.width.saturating_sub(4) as usize;
        for row in 0..area.height.saturating_sub(2) {
            let y = area.y + 1 + row;
            set_cell(buf, left, y, '\u{2502}', border_style);
            set_cell(buf, right, y, '\u{2502}', border_style);
        }
        let text = format!(
            "Zezwoli\u{0107} na {} {}? [t/n]",
            question.tool, question.arg
        );
        put_text(
            buf,
            inner_x + 1,
            area.y + 1,
            &text,
            Style::default().fg(YELLOW).bg(BG).add_modifier(Modifier::BOLD),
            text_w as u16,
        );
        if area.height > 3 {
            put_text(
                buf,
                inner_x + 1,
                area.y + 2,
                "t: zezwol   n: odmow (USER_DENIED)",
                Style::default().fg(GRAY).bg(BG),
                text_w as u16,
            );
        }
        for x in left..=right {
            let ch = if x == left {
                '\u{2570}'
            } else if x == right {
                '\u{256F}'
            } else {
                '\u{2500}'
            };
            set_cell(buf, x, bottom_y, ch, border_style);
        }
        let info = format!(" {} \u{00B7} ask ", question.tool);
        let info_w = info.chars().count() as u16;
        if area.width > info_w + 3 {
            put_text(
                buf,
                right - info_w - 1,
                bottom_y,
                &info,
                Style::default().fg(GRAY_DIM).bg(BG),
                info_w,
            );
        }
    }

    /// Podpowiedzi `/` nad composerem: filtr po prefiksie, zaznaczenie
    /// `bg_light`, `Tab`/`Enter` uzupełnia, `Esc` chowa.
    fn draw_hints(&self, buf: &mut Buffer, area: Rect) {
        let hints = self.hints();
        if hints.is_empty() || area.height == 0 || area.width < 8 {
            return;
        }
        let count = (hints.len() as u16).min(area.height);
        let name_w = hints.iter().map(|c| c.name.chars().count()).max().unwrap_or(0);
        let selected = self.hints_sel.min(hints.len().saturating_sub(1));
        for (i, command) in hints.iter().take(count as usize).enumerate() {
            let y = area.y + i as u16;
            let is_selected = i == selected;
            let style = if is_selected {
                Style::default().fg(FG).bg(BG_LIGHT)
            } else {
                Style::default().fg(FG_DARK).bg(BG_TERMINAL)
            };
            for cx in area.x..area.x + area.width {
                set_cell(buf, cx, y, ' ', style);
            }
            let text = format!(
                "  {:<width$}  {}",
                command.name,
                command.about,
                width = name_w
            );
            put_text(buf, area.x, y, &text, style, area.width);
        }
    }

    /// Stopka: po lewej pary `klawisz: opis` z separatorem ` │ ` (ten sam format
    /// i to samo miejsce co w zrzucie), po prawej — przygaszony stan przebiegu
    /// (`kroki 4/12 · narzedzia 2 · DONE`) z realnych liczników pętli.
    fn draw_footer(&self, buf: &mut Buffer, area: Rect) {
        buf.set_style(area, Style::default().bg(BG));
        if let Some(toast) = &self.toast {
            put_text(
                buf,
                area.x,
                area.y,
                toast,
                Style::default().fg(YELLOW).bg(BG),
                area.width,
            );
            return;
        }
        let (state, state_style) = match (&self.status, self.running) {
            (_, true) => ("pracuje".to_string(), Style::default().fg(TEAL)),
            (Some(status), false) if status == "DONE" => {
                (status.clone(), Style::default().fg(GREEN))
            }
            (Some(status), false) if status.starts_with("FAILED") => {
                (status.clone(), Style::default().fg(RED))
            }
            (Some(status), false) => (status.clone(), Style::default().fg(YELLOW)),
            (None, false) => ("gotowy".to_string(), Style::default().fg(GRAY_DIM)),
        };
        let dim = Style::default().fg(GRAY_DIM).bg(BG);
        let summary = format!(
            "kroki {}/{} \u{00B7} narzedzia {} \u{00B7} skille {} \u{00B7} odmowy {} \u{00B7} {}",
            self.steps,
            self.max_steps,
            self.tool_calls,
            self.skills.len(),
            self.denied,
            state
        );
        let summary_w = summary.chars().count() as u16;
        let state_w = state.chars().count() as u16;
        // Prawa strefa dostaje miejsce tylko wtedy, gdy nie zjada podpowiedzi.
        let show_summary = area.width > summary_w + 24;
        let hints_room = if show_summary {
            area.width.saturating_sub(summary_w + 2)
        } else {
            area.width
        };

        let hints: &[(&str, &str)] = if self.running {
            &[
                ("Ctrl+F", "szukaj"),
                ("Ctrl+L", "clear"),
                ("Ctrl+C", "quit"),
            ]
        } else {
            &[
                ("Enter", "send"),
                ("Ctrl+F", "szukaj"),
                ("Ctrl+L", "clear"),
                ("Ctrl+C", "quit"),
            ]
        };
        let key_style = Style::default()
            .fg(GRAY_BRIGHT)
            .bg(BG)
            .add_modifier(Modifier::BOLD);
        let action_style = Style::default().fg(GRAY).bg(BG);
        let sep_style = Style::default().fg(GRAY_DIM).bg(BG);
        let mut x = area.x;
        for (i, (key, label)) in hints.iter().enumerate() {
            if i > 0 {
                let used = x - area.x;
                x += put_text(
                    buf,
                    x,
                    area.y,
                    HINT_SEPARATOR,
                    sep_style,
                    hints_room.saturating_sub(used),
                );
            }
            let used = x - area.x;
            x += put_text(buf, x, area.y, key, key_style, hints_room.saturating_sub(used));
            let used = x - area.x;
            x += put_text(
                buf,
                x,
                area.y,
                ":",
                action_style,
                hints_room.saturating_sub(used),
            );
            let used = x - area.x;
            x += put_text(
                buf,
                x,
                area.y,
                label,
                action_style,
                hints_room.saturating_sub(used),
            );
            if x >= area.x + hints_room {
                // Podpowiedzi zajęły swoje miejsce — status po prawej i tak
                // ma osobny pas, więc rysujemy go dalej (runda 16).
                break;
            }
        }

        if show_summary {
            // Stan (`DONE`/`FAILED_*`) zostaje w swoim kolorze, reszta przygaszona.
            let plain: String = summary
                .chars()
                .take(summary.chars().count().saturating_sub(state_w as usize))
                .collect();
            let x = area.x + area.width - summary_w;
            let used = put_text(buf, x, area.y, &plain, dim, summary_w);
            put_text(buf, x + used, area.y, &state, state_style, summary_w - used);
        }
    }
}

// ── Ekran startowy: lista sesji (`views/dashboard`) ─────────────────────────

/// Pozycja na liście: nagłówek grupy albo wiersz sesji (indeks w `visible`).
enum ListItem {
    Group(&'static str, usize),
    Row(usize),
}

impl ListItem {
    fn height(&self) -> u16 {
        match self {
            ListItem::Group(_, _) => GROUP_HEADER_HEIGHT,
            ListItem::Row(_) => ROW_HEIGHT,
        }
    }
}

impl App {
    fn list_items(&self, rows: &[&SessionRow]) -> Vec<ListItem> {
        let mut items = Vec::new();
        let mut last: Option<RowState> = None;
        for (index, row) in rows.iter().enumerate() {
            if last != Some(row.state) {
                let count = rows.iter().filter(|r| r.state == row.state).count();
                items.push(ListItem::Group(row.state.group_label(), count));
                last = Some(row.state);
            }
            items.push(ListItem::Row(index));
        }
        items
    }

    fn draw_list(&self, buf: &mut Buffer, area: Rect) {
        let panes = list_layout(area);
        let now = sessions::now_secs();
        let rows = self.visible_sessions();
        self.draw_list_header(buf, panes.header, &rows);
        self.draw_list_actions(buf, panes.actions);
        self.draw_list_rows(buf, panes.list, &rows, now);
        self.draw_peek(buf, panes.peek, rows.get(self.selected).copied(), now);
        self.draw_list_dispatch(buf, panes.dispatch);
        self.draw_list_footer(buf, panes.footer);
    }

    /// Nagłówek: po lewej gałąź + badge worktree + skrócona ścieżka
    /// (`render_header`), po prawej liczniki stanów z separatorem ` │ `.
    fn draw_list_header(&self, buf: &mut Buffer, area: Rect, rows: &[&SessionRow]) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        buf.set_style(area, Style::default().bg(BG));
        let dim = Style::default().fg(GRAY_DIM).bg(BG);
        let secondary = Style::default().fg(FG_DARK).bg(BG);

        // Liczniki: tylko niezerowe grupy, w kolejności oryginału.
        let mut chips: Vec<(String, Color)> = Vec::new();
        for (state, glyph) in [
            (RowState::NeedsInput, BULLET),
            (RowState::Running, BULLET),
            (RowState::Idle, DIAMOND_HOLLOW),
            (RowState::Done, BULLET),
            (RowState::Failed, BULLET),
        ] {
            let count = rows.iter().filter(|r| r.state == state).count();
            if count == 0 {
                continue;
            }
            let color = match state {
                RowState::NeedsInput => YELLOW,
                RowState::Running => MAGENTA,
                RowState::Idle => GRAY_DIM,
                RowState::Done => GREEN,
                RowState::Failed => RED,
            };
            let label = match state {
                RowState::NeedsInput => "czeka",
                RowState::Running => "w toku",
                RowState::Idle => "bezczynna",
                RowState::Done => "gotowe",
                RowState::Failed => "blad",
            };
            chips.push((format!("{} {} {}", glyph, count, label), color));
        }
        let chips_w: u16 = chips
            .iter()
            .map(|(text, _)| text.chars().count() as u16)
            .sum::<u16>()
            + (chips.len().saturating_sub(1) as u16) * 3;
        let mut x = area.x;
        let right_edge = area.x + area.width;
        let mut budget = area.width.saturating_sub(chips_w.saturating_add(3));
        if budget < 8 {
            budget = area.width;
        }

        // Lewa strona: gałąź (przygaszona), badge `worktree`, ścieżka.
        let mut left = String::new();
        if let Some(branch) = rows.iter().find_map(|r| r.subtitle.as_deref()) {
            left.push_str(branch);
            left.push(' ');
        }
        if rows.iter().any(|r| r.badges.contains(&Badge::Worktree)) {
            left.push_str("worktree ");
        }
        let cwd = rows
            .iter()
            .map(|r| r.cwd.as_str())
            .next()
            .unwrap_or("")
            .to_string();
        left.push_str(&cwd);
        let painted = put_text(buf, x, area.y, &left, secondary, budget);
        x += painted;
        let _ = x;

        // Prawa strona: `chip │ chip │ …`, wyrównane do prawej.
        if chips_w > 0 && chips_w < area.width {
            let mut cx = right_edge - chips_w;
            for (i, (text, color)) in chips.iter().enumerate() {
                if i > 0 {
                    cx += put_text(buf, cx, area.y, SEPARATOR, dim, 3);
                }
                cx += put_text(buf, cx, area.y, text, Style::default().fg(*color).bg(BG), area.width);
            }
        }
    }

    /// Wiersz akcji: `+ Nowa sesja` po lewej, filtr po prawej (`render_actions_row`).
    fn draw_list_actions(&self, buf: &mut Buffer, area: Rect) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        buf.set_style(area, Style::default().bg(BG));
        // Poniżej `MIN_DASHBOARD_WIDTH` wiersz akcji zwija się do skrótów:
        // filtr musi zostać widoczny, bo `/` zmienia to, co użytkownik widzi.
        let narrow = area.width < MIN_DASHBOARD_WIDTH;
        let left = if narrow { "+ nowa" } else { "+ Nowa sesja" };
        put_text(
            buf,
            area.x,
            area.y,
            left,
            Style::default().fg(FG_DARK).bg(BG),
            area.width,
        );
        let right = if narrow {
            format!("/ {}", self.filter.label())
        } else {
            format!("Filtr: {}  /", self.filter.label())
        };
        let w = right.chars().count() as u16;
        if w + left.chars().count() as u16 + 2 <= area.width {
            put_text(
                buf,
                area.x + area.width - w,
                area.y,
                &right,
                Style::default().fg(GRAY).bg(BG),
                w,
            );
        }
    }

    fn draw_list_rows(&self, buf: &mut Buffer, area: Rect, rows: &[&SessionRow], now: u64) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        buf.set_style(area, Style::default().bg(BG));
        if rows.is_empty() {
            let text = if self.sessions.is_empty() {
                "Brak sesji w .agent/sessions — `n` zaczyna nowa."
            } else {
                "Filtr nic nie zostawil — `/` przelacza."
            };
            put_text(
                buf,
                area.x,
                area.y,
                text,
                Style::default().fg(GRAY).bg(BG),
                area.width,
            );
            return;
        }

        let items = self.list_items(rows);
        // Wiersz zaznaczony → jego pozycja w liniach; okno przesuwa się tak,
        // żeby zaznaczenie zostało widoczne.
        let mut tops = Vec::with_capacity(items.len());
        let mut total = 0u16;
        for item in &items {
            tops.push(total);
            total = total.saturating_add(item.height());
        }
        let selected_item = items
            .iter()
            .position(|item| matches!(item, ListItem::Row(i) if *i == self.selected))
            .unwrap_or(0);
        let selected_top = tops.get(selected_item).copied().unwrap_or(0);
        let selected_h = items
            .get(selected_item)
            .map(|item| item.height())
            .unwrap_or(ROW_HEIGHT);
        let mut offset = 0u16;
        if selected_top + selected_h > area.height {
            offset = selected_top + selected_h - area.height;
        }
        let max_offset = total.saturating_sub(area.height);
        let offset = offset.min(max_offset);

        let narrow = area.width < MIN_DASHBOARD_WIDTH;
        for (index, item) in items.iter().enumerate() {
            let top = tops[index];
            let height = item.height();
            if top + height <= offset || top >= offset + area.height {
                continue;
            }
            let y = area.y + top.saturating_sub(offset);
            // Wiersz może być przycięty od góry, gdy okno wypadło w środku.
            let visible_top = if top < offset { offset - top } else { 0 };
            let visible_h = height.saturating_sub(visible_top).min(area.y + area.height - y);
            let rect = Rect {
                x: area.x,
                y: y + visible_top,
                width: area.width,
                height: visible_h,
            };
            match item {
                ListItem::Group(label, count) => {
                    self.draw_group_header(buf, rect, label, *count);
                }
                ListItem::Row(row_index) => {
                    if let Some(row) = rows.get(*row_index) {
                        self.draw_session_row(
                            buf,
                            rect,
                            row,
                            *row_index == self.selected,
                            now,
                            narrow,
                        );
                    }
                }
            }
        }
    }

    /// `▾ Etykieta N ────` — `render_group_header` z dashboardu.
    fn draw_group_header(&self, buf: &mut Buffer, area: Rect, label: &str, count: usize) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let label_style = Style::default()
            .fg(GRAY_BRIGHT)
            .bg(BG)
            .add_modifier(Modifier::BOLD);
        let count_style = Style::default().fg(GRAY_DIM).bg(BG);
        let rule_style = Style::default().fg(SELECTION_BORDER).bg(BG);
        let mut x = area.x;
        x += put_text(
            buf,
            x,
            area.y,
            &format!("{} ", DISCLOSURE_OPEN),
            label_style,
            2,
        );
        x += put_text(buf, x, area.y, label, label_style, area.width.saturating_sub(x - area.x));
        x += put_text(
            buf,
            x,
            area.y,
            &format!(" {}", count),
            count_style,
            area.width.saturating_sub(x - area.x),
        );
        if x + 1 < area.x + area.width {
            let rule: String = "\u{2500}".repeat((area.x + area.width - x - 1) as usize);
            put_text(
                buf,
                x + 1,
                area.y,
                &rule,
                rule_style,
                area.width.saturating_sub(x + 1 - area.x),
            );
        }
    }

    /// Jeden wiersz sesji: `▏ ◆ tytuł · subtitle  badge · badge     wiek`,
    /// druga linia z aktywnością, trzeci wiersz oddechu (`render_row`).
    fn draw_session_row(
        &self,
        buf: &mut Buffer,
        area: Rect,
        row: &SessionRow,
        selected: bool,
        now: u64,
        narrow: bool,
    ) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        let bg = if selected { BG_LIGHT } else { BG };
        for x in area.x..area.x + area.width {
            for y in area.y..area.y + area.height {
                set_cell(buf, x, y, ' ', Style::default().bg(bg));
            }
        }
        // Marker: `▏` na każdym wierszu treści zaznaczonego wpisu.
        if selected {
            for y in area.y..area.y + area.height.min(2) {
                set_cell(buf, area.x, y, SELECTION_BAR, Style::default().fg(FG_DARK).bg(bg));
            }
        }
        let icon = match row.state {
            RowState::Running => {
                SPINNER[(self.tick_count / SPINNER_DIVISOR) as usize % SPINNER.len()].to_string()
            }
            RowState::Idle => DIAMOND_HOLLOW.to_string(),
            _ => BULLET.to_string(),
        };
        let icon_color = match row.state {
            RowState::NeedsInput => YELLOW,
            RowState::Running => MAGENTA,
            RowState::Idle => GRAY_DIM,
            RowState::Done => GREEN,
            RowState::Failed => RED,
        };
        put_text(
            buf,
            area.x + 2,
            area.y,
            &icon,
            Style::default().fg(icon_color).bg(bg),
            1,
        );

        let content_x = area.x + 4;
        let content_w = area.width.saturating_sub(4);
        let age = sessions::format_age(row.age_secs(now));
        let age_w = age.chars().count() as u16;
        let meta_x = area.x + area.width.saturating_sub(age_w.saturating_add(1));
        if meta_x > content_x {
            put_text(
                buf,
                meta_x,
                area.y,
                &age,
                Style::default().fg(GRAY).bg(bg),
                age_w,
            );
        }

        // Badge'y siedzą przy prawym końcu tytułu, oddzielone odstępem
        // (`RowTitle::reserve_chips`).
        let badge_text = if narrow { String::new() } else { row.badges_text() };
        let badge_w = badge_text.chars().count() as u16;
        let mut title_right = meta_x.saturating_sub(1);
        if badge_w > 0 && title_right > content_x + badge_w + 1 {
            let badges_x = title_right - badge_w;
            let mut bx = badges_x;
            for (i, badge) in row.badges.iter().enumerate() {
                if i > 0 {
                    bx += put_text(
                        buf,
                        bx,
                        area.y,
                        " · ",
                        Style::default().fg(GRAY_DIM).bg(bg),
                        3,
                    );
                }
                let color = match badge {
                    Badge::NeedsInput => YELLOW,
                    Badge::Failed => RED,
                    Badge::Worktree => GRAY,
                };
                bx += put_text(
                    buf,
                    bx,
                    area.y,
                    badge.label(),
                    Style::default().fg(color).bg(bg),
                    area.width,
                );
            }
            title_right = badges_x.saturating_sub(1);
        }

        let title_w = title_right.saturating_sub(content_x);
        if title_w > 0 {
            let mut x = content_x;
            let label = truncate(&row.title, title_w as usize);
            x += put_text(
                buf,
                x,
                area.y,
                &label,
                Style::default().fg(FG).bg(bg),
                title_w,
            );
            if !narrow {
                if let Some(subtitle) = &row.subtitle {
                    let room = title_right.saturating_sub(x);
                    if room > 3 {
                        let text = truncate(&format!(" · {}", subtitle), room as usize);
                        put_text(
                            buf,
                            x,
                            area.y,
                            &text,
                            Style::default().fg(GRAY_DIM).bg(bg),
                            room,
                        );
                    }
                }
            }
        }

        // Druga linia: ostatnia aktywność. Zaznaczony wiersz jaśnieje,
        // `Pending:` świeci na żółto (`row_activity.rs` + `render_row`).
        if area.height >= 2 {
            // Brak aktywności w transkrypcie → liczniki sesji (prawdziwe dane,
            // nie zmyślony opis).
            let counters = format!(
                "kroki {} · narzedzia {} · odmowy {}",
                row.steps, row.tool_calls, row.denied
            );
            let activity = row.activity.as_deref().unwrap_or(&counters);
            {
                let avail = content_w.saturating_sub(1);
                let text = truncate(activity, avail as usize);
                let style = if selected {
                    Style::default().fg(FG_DARK).bg(bg)
                } else {
                    Style::default().fg(GRAY_DIM).bg(bg)
                };
                if let Some(rest) = text.strip_prefix("Pending:") {
                    let prefix_w = put_text(
                        buf,
                        content_x,
                        area.y + 1,
                        "Pending:",
                        Style::default().fg(YELLOW).bg(bg),
                        avail,
                    );
                    put_text(
                        buf,
                        content_x + prefix_w,
                        area.y + 1,
                        rest,
                        style,
                        avail.saturating_sub(prefix_w),
                    );
                } else {
                    put_text(buf, content_x, area.y + 1, &text, style, avail);
                }
            }
        }
    }

    /// Panel peek po prawej stronie listy — port `views/dashboard/peek.rs`.
    /// Pokazuje status i ostatnie linie **prawdziwego** transkryptu
    /// zaznaczonej sesji; bez sesji rysuje pusty panel z podpisem.
    fn draw_peek(&self, buf: &mut Buffer, area: Rect, row: Option<&SessionRow>, now: u64) {
        if area.width < 12 || area.height < 3 {
            return;
        }
        buf.set_style(area, Style::default().bg(BG));
        let border = Style::default().fg(PROMPT_BORDER_ACTIVE).bg(BG);
        let right = area.x + area.width - 1;
        let bottom = area.y + area.height - 1;
        for x in area.x..=right {
            let (top, bot) = if x == area.x {
                ('\u{256D}', '\u{2570}')
            } else if x == right {
                ('\u{256E}', '\u{256F}')
            } else {
                ('\u{2500}', '\u{2500}')
            };
            set_cell(buf, x, area.y, top, border);
            set_cell(buf, x, bottom, bot, border);
        }
        for y in area.y + 1..bottom {
            set_cell(buf, area.x, y, '\u{2502}', border);
            set_cell(buf, right, y, '\u{2502}', border);
        }
        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        if inner.width < 4 || inner.height == 0 {
            return;
        }

        let preview = row.map(|row| {
            let events = sessions::transcript(&self.workspace, &row.id);
            peek_preview(&events, now, &row.title)
        });
        let empty = PeekPreview {
            label: "podglad".to_string(),
            age: String::new(),
            status: "brak sesji".to_string(),
            status_color: GRAY_DIM,
            lines: Vec::new(),
        };
        let preview = preview.as_ref().unwrap_or(&empty);

        // Tytuł sesji na górnej krawędzi; podpis panelu na dolnej — jak badge
        // modelu na ramce peeku w oryginale.
        let title = format!(" {} ", truncate(&preview.label, inner.width.saturating_sub(4) as usize));
        put_text(
            buf,
            area.x + 2,
            area.y,
            &title,
            Style::default().fg(GRAY).bg(BG),
            inner.width.saturating_sub(2),
        );
        let foot = " peek ";
        let foot_w = foot.chars().count() as u16;
        if foot_w + 2 <= inner.width {
            put_text(
                buf,
                area.x + area.width - foot_w - 2,
                bottom,
                foot,
                Style::default().fg(GRAY_DIM).bg(BG),
                foot_w,
            );
        }

        // Status (typ ostatniej odpowiedzi) po lewej, wiek po prawej —
        // ten sam układ co wiersz statusu peeku.
        put_text(
            buf,
            inner.x,
            inner.y,
            &truncate(&preview.status, inner.width as usize),
            Style::default().fg(preview.status_color).bg(BG),
            inner.width,
        );
        let age_w = preview.age.chars().count() as u16;
        if age_w > 0 && age_w + 2 <= inner.width {
            put_text(
                buf,
                inner.x + inner.width - age_w,
                inner.y,
                &preview.age,
                Style::default().fg(GRAY_DIM).bg(BG),
                age_w,
            );
        }

        let body_h = inner.height.saturating_sub(1);
        if body_h == 0 {
            return;
        }
        if preview.lines.is_empty() {
            put_text(
                buf,
                inner.x,
                inner.y + 1,
                "brak wpisow w transkrypcie",
                Style::default().fg(GRAY_DIM).bg(BG),
                inner.width,
            );
            return;
        }
        // Jeden pusty wiersz oddechu, gdy zostaje na niego miejsce
        // (`peek_live_tail_desired_content`: `blank_row`).
        let cap = if body_h >= 3 { body_h - 1 } else { body_h } as usize;
        let start = preview.lines.len().saturating_sub(cap);
        let shown = &preview.lines[start..];
        let top = inner.y + 1 + body_h.saturating_sub(shown.len() as u16);
        for (i, line) in shown.iter().enumerate() {
            let y = top + i as u16;
            if y >= bottom {
                break;
            }
            let mut style = Style::default().fg(line.color).bg(BG);
            if line.bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            put_text(
                buf,
                inner.x,
                y,
                &truncate(&line.text, inner.width as usize),
                style,
                inner.width,
            );
        }
    }

    /// Pole nowego zadania pod listą — `compute_layout` daje mu 3 wiersze
    /// (ramka + tekst), na bardzo niskim terminalu 1.
    fn draw_list_dispatch(&self, buf: &mut Buffer, area: Rect) {
        if area.height == 0 || area.width < 6 {
            return;
        }
        buf.set_style(area, Style::default().bg(BG));
        let border = if self.input.is_empty() {
            PROMPT_BORDER
        } else {
            PROMPT_BORDER_ACTIVE
        };
        let border_style = Style::default().fg(border).bg(BG);
        let left = area.x;
        let right = area.x + area.width - 1;
        let text_y = area.y + area.height / 2;
        if area.height >= 3 {
            for x in left..=right {
                let ch = if x == left {
                    '\u{256D}'
                } else if x == right {
                    '\u{256E}'
                } else {
                    '\u{2500}'
                };
                set_cell(buf, x, area.y, ch, border_style);
            }
            for x in left..=right {
                let ch = if x == left {
                    '\u{2570}'
                } else if x == right {
                    '\u{256F}'
                } else {
                    '\u{2500}'
                };
                set_cell(buf, x, area.y + area.height - 1, ch, border_style);
            }
        }
        set_cell(buf, left, text_y, '\u{2502}', border_style);
        set_cell(buf, right, text_y, '\u{2502}', border_style);
        let inner_x = left + 1;
        let inner_w = area.width.saturating_sub(4);
        put_text(
            buf,
            inner_x,
            text_y,
            PROMPT_ARROW,
            Style::default().fg(FG_DARK).bg(BG),
            2,
        );
        let text_w = inner_w.saturating_sub(2);
        let shown = truncate(&self.input, text_w as usize);
        put_text(buf, inner_x + 2, text_y, &shown, Style::default().fg(FG).bg(BG), text_w);
        if self.input.is_empty() {
            put_text(
                buf,
                inner_x + 2,
                text_y,
                "Nowe zadanie… (Enter startuje, n nowa sesja)",
                Style::default().fg(GRAY).bg(BG),
                text_w,
            );
        }
    }

    fn draw_list_footer(&self, buf: &mut Buffer, area: Rect) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        buf.set_style(area, Style::default().bg(BG));
        let key_style = Style::default()
            .fg(GRAY_BRIGHT)
            .bg(BG)
            .add_modifier(Modifier::BOLD);
        let action_style = Style::default().fg(GRAY).bg(BG);
        let sep_style = Style::default().fg(GRAY_DIM).bg(BG);
        let hints: &[(&str, &str)] = &[
            ("\u{2191}\u{2193}", "wybor"),
            ("Enter", "otworz"),
            ("n", "nowa sesja"),
            ("/", "filtr"),
            ("Esc", "wyjscie"),
        ];
        let mut x = area.x;
        for (i, (key, label)) in hints.iter().enumerate() {
            if i > 0 {
                x += put_text(
                    buf,
                    x,
                    area.y,
                    HINT_SEPARATOR,
                    sep_style,
                    area.width.saturating_sub(x - area.x),
                );
            }
            x += put_text(
                buf,
                x,
                area.y,
                key,
                key_style,
                area.width.saturating_sub(x - area.x),
            );
            x += put_text(
                buf,
                x,
                area.y,
                ": ",
                action_style,
                area.width.saturating_sub(x - area.x),
            );
            x += put_text(
                buf,
                x,
                area.y,
                label,
                action_style,
                area.width.saturating_sub(x - area.x),
            );
            if x >= area.x + area.width {
                return;
            }
        }
    }
}

// ── Panel peek: dane z transkryptu (`views/dashboard/peek.rs`) ─────────────

/// Jedna linia tailu podglądu: tekst, kolor i ewentny bold.
struct PeekLine {
    text: String,
    color: Color,
    bold: bool,
}

impl PeekLine {
    fn new(text: String, color: Color) -> PeekLine {
        PeekLine {
            text,
            color,
            bold: false,
        }
    }

    fn heavy(text: String, color: Color) -> PeekLine {
        PeekLine {
            text,
            color,
            bold: true,
        }
    }
}

/// `PeekFields` + tail: status ostatniej odpowiedzi, wiek i ostatnie linie
/// prawdziwego transkryptu.
struct PeekPreview {
    label: String,
    age: String,
    status: String,
    status_color: Color,
    lines: Vec<PeekLine>,
}

/// Pierwsza niepusta linia tekstu, przycięta do `limit` znaków.
fn first_line(text: &str, limit: usize) -> String {
    let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    short(line.trim(), limit)
}

/// Linie podglądu z eventów: zadanie, myśl, wywołanie, wynik, odmowa,
/// podsumowanie. Nieznane rodzaje eventów są pomijane — panel nie zmyśla.
fn peek_preview(events: &[Value], now: u64, fallback_label: &str) -> PeekPreview {
    let mut lines: Vec<PeekLine> = Vec::new();
    let mut label = fallback_label.to_string();
    let mut last_at = now;
    let mut subject: Option<String> = None;
    for event in events {
        let kind = event.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let data = event.get("data").cloned().unwrap_or(Value::Null);
        if let Some(at) = event.get("at").and_then(|v| v.as_u64()) {
            last_at = at;
        }
        match kind {
            "task" => {
                let text = first_line(&str_field(&data, "text"), 120);
                if !text.is_empty() {
                    label = text.clone();
                }
                lines.push(PeekLine::new(
                    format!("{}{}", PROMPT_ARROW, text),
                    ACCENT_USER,
                ));
            }
            "thought" => {
                let step = num_field(&data, "step");
                let thought = first_line(&str_field(&data, "thought"), 120);
                let head = if step > 0 {
                    format!("{} Thought  krok {}", BULLET, step)
                } else {
                    format!("{} Thought", BULLET)
                };
                lines.push(PeekLine::new(
                    if thought.is_empty() {
                        head
                    } else {
                        format!("{}  {}", head, thought)
                    },
                    GRAY,
                ));
            }
            "tool_call" => {
                let tool = str_field(&data, "tool");
                let args = data.get("args").cloned().unwrap_or(Value::Null);
                let display = tool_display(&tool, &args);
                subject = Some(display.subject.clone());
                lines.push(PeekLine::new(
                    format!("{} {} {}", BULLET, display.verb_present, display.subject),
                    MAGENTA,
                ));
            }
            "tool_result" => {
                let tool = str_field(&data, "tool");
                let ok = data.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                let bytes = num_field(&data, "bytes");
                let code = str_field(&data, "code");
                let what = subject.clone().unwrap_or_else(|| tool.clone());
                if ok {
                    lines.push(PeekLine::new(
                        format!(
                            "{} {} {}  +{} B",
                            CHECK_MARK,
                            past_verb(&tool),
                            what,
                            bytes
                        ),
                        GRAY,
                    ));
                } else {
                    lines.push(PeekLine::new(
                        format!(
                            "{} {} {}  {}",
                            BALLOT_X,
                            past_verb(&tool),
                            what,
                            if code.is_empty() {
                                "ERROR".to_string()
                            } else {
                                code
                            }
                        ),
                        RED,
                    ));
                }
            }
            "tool_result_cached" => {
                let tool = str_field(&data, "tool");
                let what = subject.clone().unwrap_or_else(|| tool.clone());
                lines.push(PeekLine::new(
                    format!("{} {} {}  z zapisu", CHECK_MARK, past_verb(&tool), what),
                    GRAY,
                ));
            }
            "tool_result_skipped" => {
                let tool = str_field(&data, "tool");
                let reason = str_field(&data, "reason");
                let what = subject.clone().unwrap_or_else(|| tool.clone());
                lines.push(PeekLine::new(
                    format!(
                        "{} {} {}  pominiety ({})",
                        BULLET,
                        past_verb(&tool),
                        what,
                        reason
                    ),
                    YELLOW,
                ));
            }
            "denied" => {
                let tool = str_field(&data, "tool");
                let reason = str_field(&data, "reason");
                lines.push(PeekLine::new(
                    format!("{} Odmowa {} \u{2014} {}", BALLOT_X, tool, reason),
                    RED,
                ));
            }
            "summary" => {
                let status = str_field(&data, "status");
                let status = if status.is_empty() {
                    "DONE".to_string()
                } else {
                    status
                };
                let color = if status == "DONE" { GREEN } else { RED };
                lines.push(PeekLine::heavy(format!("{} {}", BULLET, status), color));
            }
            "error" => {
                let message = first_line(&str_field(&data, "message"), 120);
                lines.push(PeekLine::new(format!("{} {}", BALLOT_X, message), RED));
            }
            _ => {}
        }
    }
    // `MAX_LIVE_TAIL_ROWS`: panel pokazuje ogon, nie całą sesję.
    let limit = MAX_LIVE_TAIL_ROWS as usize;
    if lines.len() > limit {
        lines.drain(0..lines.len() - limit);
    }
    let (status, status_color) = last_status(events);
    PeekPreview {
        label,
        age: sessions::format_age(now.saturating_sub(last_at.min(now))),
        status,
        status_color,
        lines,
    }
}

/// Typ ostatniej odpowiedzi (`extract_last_response_type`): od ostatniego
/// podsumowania idziemy wstecz do pierwszego bloku, który coś mówi.
fn last_status(events: &[Value]) -> (String, Color) {
    for event in events.iter().rev() {
        let kind = event.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let data = event.get("data").cloned().unwrap_or(Value::Null);
        let found = match kind {
            "summary" => {
                let status = str_field(&data, "status");
                let status = if status.is_empty() {
                    "DONE".to_string()
                } else {
                    status
                };
                let color = if status == "DONE" { GREEN } else { RED };
                Some((status, color))
            }
            "error" => Some(("Blad".to_string(), RED)),
            "denied" => Some(("Odmowa".to_string(), YELLOW)),
            "tool_result" => Some((past_verb(&str_field(&data, "tool")).to_string(), GRAY)),
            "tool_result_cached" => Some(("z zapisu".to_string(), GRAY)),
            "tool_result_skipped" => Some(("pominiety".to_string(), YELLOW)),
            "tool_call" => Some((present_verb(&str_field(&data, "tool")).to_string(), MAGENTA)),
            "thought" => Some(("Thought".to_string(), GRAY)),
            "task" => Some(("Idle".to_string(), GRAY_DIM)),
            _ => None,
        };
        if let Some(found) = found {
            return found;
        }
    }
    ("Idle".to_string(), GRAY_DIM)
}

// ── Wpisy → wiersze ─────────────────────────────────────────────────────────

/// Jeden wiersz scrollbacku. Treść idzie od lewej krawędzi obszaru (bez
/// kolumny akcentu i bez wcięcia bloku — tak jak na zrzucie `readme-tui.png`),
/// a `right` to tekst wyrównany do prawej tego samego wiersza.
struct Row {
    /// Tło całego wiersza: pas zadania (`bg_light`) i wiersze diffu.
    bg: Option<Color>,
    /// Tekst przy prawej krawędzi (znacznik czasu w pasie zadania).
    right: Option<(String, Style)>,
    spans: Vec<Span<'static>>,
}

impl Row {
    fn plain(spans: Vec<Span<'static>>) -> Row {
        Row {
            bg: None,
            right: None,
            spans,
        }
    }

    fn banded(spans: Vec<Span<'static>>) -> Row {
        Row {
            bg: Some(BG_LIGHT),
            right: None,
            spans,
        }
    }
}

/// Skrót do dokładnie `max` znaków (wielokropek wliczony) — eksport ma
/// twardy limit 400 znaków na wynik, nie „400 plus kropki”.
fn clip_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(3)).collect();
    cut.push_str("...");
    cut
}

/// Cały tekst wiersza (spany sklejone) — po nim liczy się trafienia szukania.
fn row_text(row: &Row) -> String {
    row.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// Tekst wpisu do eksportu i schowka: nagłówek + treść, bez ramek i znaków
/// bloków (`◆`, `❯`).
fn entry_text(entry: &Entry) -> String {
    match &entry.body {
        Some(body) if !body.trim().is_empty() => {
            format!("{}\n{}", entry.subject, body)
        }
        _ => entry.subject.clone(),
    }
}

/// Jednolinijkowy wynik narzędzia do eksportu: ścieżka, rozmiar, podgląd.
fn result_line(entry: &Entry) -> String {
    let mut text = entry.subject.clone();
    if !entry.detail.is_empty() {
        text.push(' ');
        text.push_str(&entry.detail);
    }
    if let Some(body) = &entry.body {
        if let Some(first) = body.lines().find(|line| !line.trim().is_empty()) {
            text.push_str(" \u{00B7} ");
            text.push_str(first.trim());
        }
    }
    text
}

/// Podświetlenie trafień: odwrócenie kolorów w miejscu dopasowania, bez
/// zmiany palety i bez ruszania reszty wiersza. Dopasowanie liczone jest na
/// małych literach; gdy zmiana długości przesunęłaby indeksy (np. `ß`) albo
/// trafiła w środek znaku UTF-8, span zostaje bez podświetlenia, zamiast
/// ciąć tekst w złym miejscu.
fn highlight_spans(spans: &[Span<'static>], needle: &str) -> Vec<Span<'static>> {
    if needle.is_empty() {
        return spans.to_vec();
    }
    let mut out: Vec<Span<'static>> = Vec::new();
    for span in spans {
        let text = span.content.to_string();
        let hay = text.to_lowercase();
        if hay.len() != text.len() || !hay.contains(needle) {
            out.push(span.clone());
            continue;
        }
        let mut from = 0;
        while let Some(pos) = hay[from..].find(needle) {
            let start = from + pos;
            let end = start + needle.len();
            if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                break;
            }
            if start > from {
                out.push(Span::styled(text[from..start].to_string(), span.style));
            }
            out.push(Span::styled(
                text[start..end].to_string(),
                span.style.add_modifier(Modifier::REVERSED),
            ));
            from = end;
        }
        if from < text.len() {
            out.push(Span::styled(text[from..].to_string(), span.style));
        }
    }
    out
}

/// Wcięcie tekstu w pasie zadania: spacja, `❯`, spacja (`prompt_styles`).
const BAND_INDENT: usize = 3;

fn entry_rows(entry: &Entry, width: usize, timestamps: bool, md: &markdown::Cache) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    match entry.kind {
        Kind::Task => {
            // `scrollback/blocks/user.rs`: pas `bg_light` (#242424) pod całym
            // wpisem, prefiks `❯` (`accent_user` #c8c8c8), tekst `text_primary`
            // (#e1e1e1), godzina po prawej. Ze zrzutu: jeden wiersz tekstu
            // i po jednym wierszu odstępu nad i pod (pas ≈ 3 wiersze).
            let prefix_style = Style::default().fg(ACCENT_USER).bg(BG_LIGHT);
            let body_style = Style::default().fg(FG).bg(BG_LIGHT);
            // Godzina stoi przy prawej krawędzi tego samego wiersza, więc tekst
            // nie może pod nią wejść: `8:19 AM` w oryginale, u nas `HH:MM`.
            // Ustawienie `znaczniki czasu = off` po prostu ją ukrywa.
            let stamp = if timestamps { entry.stamp.clone() } else { String::new() };
            let stamp_w = if stamp.is_empty() {
                0
            } else {
                stamp.chars().count() + 2
            };
            let tail_width = width
                .saturating_sub(BAND_INDENT)
                .saturating_sub(stamp_w)
                .max(1);
            let lines = wrap(&entry.subject, tail_width);
            rows.push(Row::banded(Vec::new()));
            let last = lines.len().saturating_sub(1);
            for (i, line) in lines.into_iter().enumerate() {
                let prefix = if i == 0 {
                    format!(" {}", PROMPT_ARROW)
                } else {
                    " ".repeat(BAND_INDENT)
                };
                let mut row = Row::banded(vec![
                    Span::styled(prefix, prefix_style),
                    Span::styled(line, body_style),
                ]);
                if i == last && !stamp.is_empty() {
                    row.right = Some((
                        stamp.clone(),
                        Style::default().fg(GRAY_DIM).bg(BG_LIGHT),
                    ));
                }
                rows.push(row);
            }
            rows.push(Row::banded(Vec::new()));
        }
        Kind::Thought => {
            // `◆ Thought for 2.0s` — romb w `accent_thinking` (magenta),
            // etykieta przygaszona. Czas tylko wtedy, gdy go zmierzono.
            let mut header = vec![Span::styled(
                format!("{} ", BULLET),
                Style::default().fg(MAGENTA),
            )];
            header.push(Span::styled("Thought", Style::default().fg(GRAY)));
            if let Some(latency) = entry.latency.filter(|d| *d >= THOUGHT_MIN) {
                header.push(Span::styled(
                    format!(" for {:.1}s", latency.as_secs_f64()),
                    Style::default().fg(GRAY_DIM),
                ));
            }
            // Runda 21: znacznik, że tekst jeszcze rośnie ze strumienia.
            if entry.live {
                header.push(Span::styled(
                    " · streaming",
                    Style::default().fg(GRAY_DIM),
                ));
            }
            rows.push(Row::plain(header));
            if !entry.subject.trim().is_empty() {
                // Runda 18: markdown tylko tutaj — to tekst asystenta.
                rows.push(Row::plain(Vec::new()));
                rows.extend(markdown_rows(md, &entry.subject, width));
            }
        }
        Kind::Call => {
            // `scrollback/blocks/tool/*`: punkt `◆`, bold czasownik, ścieżka
            // w kolorze `path`; w toku — romb w kolorze myśli.
            rows.push(Row::plain(tool_spans(entry, BULLET, MAGENTA, FG, ORANGE)));
        }
        Kind::Ok | Kind::Cached | Kind::Skipped => {
            // `ToolConfig::muted_collapsed = true`: zakończone bloki przygaszone.
            let (color, bullet) = match entry.kind {
                Kind::Skipped => (YELLOW, BULLET),
                Kind::Cached => (GRAY, CHECK_MARK),
                _ => (GRAY, BULLET),
            };
            rows.push(Row::plain(tool_spans(entry, bullet, GRAY, color, color)));
            if let Some(content) = entry.content.as_deref() {
                // `scrollback/blocks/tool/edit.rs`: pusty wiersz, potem linie
                // diffu z numerem i pasem tła (`diff_insert_bg`).
                rows.push(Row::plain(Vec::new()));
                rows.extend(diff_rows(content, width));
            } else if let Some(body) = &entry.body {
                rows.push(Row::plain(Vec::new()));
                for line in wrap(body, width).into_iter().take(4) {
                    rows.push(Row::plain(vec![Span::styled(
                        line,
                        Style::default().fg(GRAY),
                    )]));
                }
            }
        }
        Kind::Fail => {
            rows.push(Row::plain(tool_spans(entry, BULLET, RED, FG_DARK, ORANGE)));
            if let Some(body) = &entry.body {
                rows.push(Row::plain(Vec::new()));
                for line in wrap(body, width).into_iter().take(5) {
                    rows.push(Row::plain(vec![Span::styled(
                        line,
                        Style::default().fg(RED),
                    )]));
                }
            }
        }
        Kind::Denied => {
            // `views/permission_view`: blok odmowy w kolorze błędu, pełną szerokością.
            rows.push(Row::plain(vec![
                Span::styled(
                    format!("{} ", BALLOT_X),
                    Style::default().fg(RED).add_modifier(Modifier::BOLD),
                ),
                Span::styled("Odmowa", Style::default().fg(RED)),
                Span::styled(
                    format!("  {}", entry.subject),
                    Style::default().fg(FG_DARK),
                ),
            ]));
        }
        Kind::Error => {
            for line in wrap(&format!("{} {}", BALLOT_X, entry.subject), width) {
                rows.push(Row::plain(vec![Span::styled(
                    line,
                    Style::default().fg(RED),
                )]));
            }
        }
        Kind::Info => {
            for line in wrap(&format!("{} {}", DIAMOND_HOLLOW, entry.subject), width) {
                rows.push(Row::plain(vec![Span::styled(
                    line,
                    Style::default().fg(GRAY),
                )]));
            }
        }
        Kind::Summary => {
            // `scrollback/blocks/session_event.rs`: znacznik stanu, bez akcentu.
            let color = if entry.ok { GREEN } else { RED };
            let mut spans = vec![Span::styled(
                entry.subject.clone(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )];
            if !entry.detail.is_empty() {
                spans.push(Span::styled(
                    format!(" · {}", entry.detail),
                    Style::default().fg(GRAY_DIM),
                ));
            }
            rows.push(Row::plain(spans));
            if let Some(body) = &entry.body {
                // Powód zakończenia to zdarzenie sesji, nie odpowiedź modelu —
                // dosłownie, bez markdownu.
                rows.push(Row::plain(Vec::new()));
                rows.extend(literal_rows(body, width, Style::default().fg(GRAY_DIM)));
            }
        }
    }
    rows
}

/// Nagłówek wywołania narzędzia: `◆ Czasownik ścieżka  +12 B`.
fn tool_spans(
    entry: &Entry,
    bullet: char,
    bullet_color: Color,
    verb_color: Color,
    subject_color: Color,
) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(format!("{} ", bullet), Style::default().fg(bullet_color)),
        Span::styled(
            format!("{} ", entry.verb),
            Style::default()
                .fg(verb_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(entry.subject.clone(), Style::default().fg(subject_color)),
    ];
    if !entry.detail.is_empty() {
        let color = if entry.kind == Kind::Fail {
            RED
        } else {
            GRAY_DIM
        };
        spans.push(Span::styled(format!("  {}", entry.detail), Style::default().fg(color)));
    }
    spans
}

/// Odpowiedź asystenta: markdown (nagłówki, listy, kod z ramką, tabela,
/// cytat). Renderer ma własną pamięć `(tekst, szerokość)`, więc dosłownie
/// dopisywany blok przelicza się tylko raz, a nie przy każdej klatce.
fn markdown_rows(md: &markdown::Cache, text: &str, width: usize) -> Vec<Row> {
    md.get(text, width)
        .iter()
        .map(|line| Row {
            bg: line.bg,
            right: None,
            spans: line.spans.clone(),
        })
        .collect()
}

/// Dosłowny tekst tam, gdzie markdown nie obowiązuje (wynik narzędzia,
/// powód zakończenia): `**` i `#` zostają znakami, bo to czyjaś treść.
fn literal_rows(text: &str, width: usize, style: Style) -> Vec<Row> {
    wrap(text, width)
        .into_iter()
        .map(|line| Row::plain(vec![Span::styled(line, style)]))
        .collect()
}

/// Wiersze bloku diffu dla zapisanej treści: `"  " + numer + "  " + tekst`,
/// numer przygaszony, pas tła `diff_insert_bg` — `render_diff_hunks_core`
/// z `scrollback/blocks/tool/edit.rs` (nowy plik = same wstawienia).
fn diff_rows(content: &str, width: usize) -> Vec<Row> {
    let mut lines: Vec<&str> = content.split('\n').collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    if lines.is_empty() {
        lines.push("");
    }
    let digits = lines.len().max(1).to_string().len();
    lines
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let number = format!("{:>width$}", i + 1, width = digits);
            let room = width.saturating_sub(digits + 4).max(1);
            let body: String = text.chars().take(room).collect();
            Row {
                right: None,
                bg: Some(GREEN_DARK),
                spans: vec![
                    Span::styled(format!("  {}  ", number), Style::default().fg(GRAY)),
                    Span::styled(body, Style::default().fg(FG)),
                ],
            }
        })
        .collect()
}

// ── Narzędzia → nagłówek wpisu (mapa jak `scrollback/blocks/tool`) ──────────

struct ToolDisplay {
    verb_present: &'static str,
    subject: String,
    detail: String,
}

/// Miejsce w scrollbacku zajęte przez trwające wywołanie narzędzia.
struct ToolSlot {
    slot: usize,
    subject: String,
    /// Treść zapisu przeniesiona z wywołania do wyniku (dla bloku diffu).
    content: Option<String>,
}

fn past_verb(tool: &str) -> &'static str {
    match tool {
        "fs.read" => "Read",
        "fs.write" => "Wrote",
        "fs.append" => "Appended",
        "notify.send" => "Sent",
        "skill.load" => "Loaded",
        _ => "Ran",
    }
}

fn present_verb(tool: &str) -> &'static str {
    match tool {
        "fs.read" => "Reading",
        "fs.write" => "Writing",
        "fs.append" => "Appending",
        "notify.send" => "Sending",
        "skill.load" => "Loading",
        _ => "Running",
    }
}

fn tool_display(tool: &str, args: &Value) -> ToolDisplay {
    let subject = match tool {
        "fs.read" => str_field(args, "path"),
        "fs.write" => str_field(args, "path"),
        "fs.append" => str_field(args, "path"),
        "notify.send" => str_field(args, "message"),
        "skill.load" => str_field(args, "name"),
        _ => compact(args, 80),
    };
    let detail = match tool {
        "fs.write" => {
            let bytes = str_field(args, "content").len();
            format!("+{} B", bytes)
        }
        _ => String::new(),
    };
    ToolDisplay {
        verb_present: present_verb(tool),
        subject: if subject.is_empty() {
            tool.to_string()
        } else {
            subject
        },
        detail,
    }
}

// ── Rysowanie do bufora ─────────────────────────────────────────────────────

fn set_cell(buf: &mut Buffer, x: u16, y: u16, ch: char, style: Style) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char(ch);
        cell.set_style(style);
    }
}

/// Przycina tekst z lewej, żeby zmieścił się w `width` kolumnach — ścieżka
/// w górnym pasku traci początek, nie koniec (jak w zrzucie `~/Documents`).
fn trim_left(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "\u{2026}".to_string();
    }
    let tail: String = chars[chars.len() - (width - 1)..].iter().collect();
    format!("\u{2026}{}", tail)
}

/// Wpisuje tekst przycinając do `max_w` kolumn; zwraca liczbę zajętych kolumn.
fn put_text(buf: &mut Buffer, x: u16, y: u16, text: &str, style: Style, max_w: u16) -> u16 {
    let mut used = 0u16;
    for ch in text.chars() {
        if used >= max_w {
            break;
        }
        set_cell(buf, x + used, y, ch, style);
        used += 1;
    }
    used
}

pub(crate) fn paint_spans(buf: &mut Buffer, x: u16, y: u16, spans: &[Span<'static>], max_w: u16) {
    let mut used = 0u16;
    for span in spans {
        for ch in span.content.chars() {
            if used >= max_w {
                return;
            }
            set_cell(buf, x + used, y, ch, span.style);
            used += 1;
        }
    }
}

/// Wypisuje gotowe wiersze ekranu (znak + styl) w prostokącie, wiersz pod
/// wierszem — wspólne dla ekranu powitalnego i modala.
fn draw_rows(buf: &mut Buffer, area: Rect, rows: &[welcome::Row]) {
    for (i, row) in rows.iter().enumerate() {
        let y = area.y + i as u16;
        if y >= area.y + area.height {
            return;
        }
        let spans: Vec<Span<'static>> = row
            .spans()
            .into_iter()
            .map(|(text, style)| Span::styled(text, style))
            .collect();
        paint_spans(buf, area.x, y, &spans, area.width);
    }
}

// ── Pomocnicze ──────────────────────────────────────────────────────────────

/// Przesunięcie strefy lokalnej od UTC w sekundach — ustalane raz na proces
/// (`date +%z`), bo `std` nie zna stref. Brak `date` = UTC.
fn local_offset() -> i64 {
    static OFFSET: OnceLock<i64> = OnceLock::new();
    *OFFSET.get_or_init(|| {
        let out = match std::process::Command::new("date").arg("+%z").output() {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
            _ => return 0,
        };
        parse_zone(&out).unwrap_or(0)
    })
}

/// `+0200` / `-0730` → sekundy od UTC.
fn parse_zone(raw: &str) -> Option<i64> {
    let sign = match raw.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return None;
    }
    let hours: i64 = digits.get(0..2)?.parse().ok()?;
    let minutes: i64 = digits.get(2..4)?.parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `HH:MM` w czasie lokalnym z uniksowych sekund (znacznik pasa zadania).
pub(crate) fn clock(secs: u64) -> String {
    let local = secs as i64 + local_offset();
    let day = local.rem_euclid(86_400);
    format!("{:02}:{:02}", day / 3600, (day % 3600) / 60)
}

fn clock_now() -> String {
    clock(now_secs())
}

fn byte_index(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// Twarde zawijanie composera: `width` znaków na wiersz, co najmniej jeden.
/// (Terminal łamie tak samo — bez przenoszenia całych słów, jak textarea pagera
/// przy twardym zawijaniu.)
fn composer_grid(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let end = (i + width).min(chars.len());
        out.push(chars[i..end].iter().collect());
        i = end;
    }
    out
}

/// Przycina tekst do `width` kolumn (bez wielokropka — tak robi `truncate_str`).
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars().take(width).collect()
}

fn short(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let clipped: String = text.chars().take(limit).collect();
    format!("{}…", clipped)
}

fn str_field(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn num_field(data: &Value, key: &str) -> u64 {
    data.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn compact(value: &Value, limit: usize) -> String {
    let text = match value {
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| {
                let inner = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                format!("{}={}", k, inner)
            })
            .collect::<Vec<_>>()
            .join(" "),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    short(&text, limit)
}

/// Skrót wyniku narzędzia — reszta zostaje w transkrypcie.
fn preview(output: &Value, lines: usize) -> Option<String> {
    let text = match output {
        Value::Object(map) => {
            if let Some(content) = map.get("content").and_then(|v| v.as_str()) {
                content.to_string()
            } else if let Some(message) = map.get("message").and_then(|v| v.as_str()) {
                message.to_string()
            } else if let Some(error) = map.get("error").and_then(|v| v.as_str()) {
                error.to_string()
            } else {
                compact(output, 200)
            }
        }
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return None;
    }
    Some(
        cleaned
            .lines()
            .take(lines)
            .map(|l| l.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Zawijanie po szerokości znaków (ekran terminala i tak liczy kolumny).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut out = Vec::new();
    for raw in text.split('\n') {
        let chars: Vec<char> = raw.chars().collect();
        if chars.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut start = 0;
        while start < chars.len() {
            let mut end = (start + width).min(chars.len());
            if end < chars.len() {
                if let Some(pos) = chars[start..end].iter().rposition(|c| *c == ' ') {
                    if pos > 0 {
                        end = start + pos;
                    }
                }
            }
            let piece: String = chars[start..end].iter().collect();
            out.push(piece.trim_end().to_string());
            start = end;
            while start < chars.len() && chars[start] == ' ' {
                start += 1;
            }
        }
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

// ── Zrzut ekranu / demo ─────────────────────────────────────────────────────

/// Odpowiedź asystenta do zrzutów rundy 18: nagłówek, akapit, lista, blok
/// kodu, tabela i cytat — wszystko to, co renderuje markdown. Treść jest
/// przykładowa i mieści się także w 40 kolumnach.
pub const MARKDOWN_SAMPLE: &str = "\
## Plan przebiegu

Zapisuje `notatka.txt`, potem czytam plik i porownuje **kroki** oraz *wynik*.

1. zapis pliku
2. odczyt i sprawdzenie

```rust
fn main() {
    let kroki = 3; // limit
}
```

| Krok | Wynik |
|---|---|
| 1 | ok |
| 2 | ok |

> `..` poza workspace to odmowa.";

/// Ekran rozmowy z odpowiedzią w markdownie (`--screenshot-markdown`).
/// Ten sam układ co `demo`, ale treść asystenta jest stała — dzięki temu
/// zrzut pokazuje render, a nie wynik losowego przebiegu.
pub fn markdown_shot(workspace: PathBuf) -> App {
    let _ = std::fs::remove_dir_all(&workspace);
    let _ = std::fs::create_dir_all(&workspace);
    let mut opts = Options::new(workspace);
    opts.provider = Some("mock".to_string());
    opts.max_steps = 12;
    opts.mcp = false;
    let mut app = App::new(opts);
    app.catalog = models::demo_catalog();
    app.model = "mimo-v2.6-flash".to_string();

    app.push(Entry::new(
        Kind::Task,
        "",
        "write notatka.txt :: plan przebiegu; read notatka.txt",
    ));
    let mut thought = Entry::new(Kind::Thought, "", MARKDOWN_SAMPLE).step(1);
    thought.latency = Some(Duration::from_millis(2000));
    app.push(thought);
    let mut call = Entry::new(Kind::Call, "Writing", "notatka.txt")
        .detail("+38 B")
        .step(1);
    call.tool = "fs.write".to_string();
    call.running = false;
    app.push(call);
    let mut done = Entry::new(Kind::Ok, "Wrote", "notatka.txt")
        .detail("+38 B")
        .step(1);
    done.tool = "fs.write".to_string();
    app.push(done);

    app.running = false;
    app.turn_started = None;
    app.status = Some("DONE".to_string());
    app.steps = 2;
    app.tool_calls = 2;
    app.input.clear();
    app.cursor = 0;
    app.show_scrollback_start();
    app
}

/// Ten sam ekran co `markdown_shot`, ale rysowany w 40 kolumnach
/// (`--screenshot-markdown-40`) — dowód, że wąski terminal nic nie urywa.
pub fn markdown_narrow_shot(workspace: PathBuf) -> App {
    markdown_shot(workspace)
}

/// Runda 21: tekst bloku myśli w trakcie strumienia (urwany w połowie zdania,
/// znacznik `live`). Zrzut pokazuje stan, w którym model jeszcze pisze.
pub const STREAM_SAMPLE: &str = "\
Sprawdzam plik i planuję kolejny krok:

1. odczyt `notatka.txt`
2. porównanie z tym, co zapisałem

Jeśli treść się zgadza, zamykam zadanie. W przeciwnym razie dopiszę brakującą linię i sprawdzę **jeszcze raz** —";

/// Ekran w trakcie strumienia (`--screenshot-stream`): pierwszy krok zamknięty,
/// drugi rośnie na żywo. Ten sam układ co `markdown_shot`, tylko blok myśli ma
/// znacznik `streaming`, a wiersz aktywności liczy czas i znaki.
pub fn stream_shot(workspace: PathBuf) -> App {
    let _ = std::fs::remove_dir_all(&workspace);
    let _ = std::fs::create_dir_all(&workspace);
    let mut opts = Options::new(workspace);
    opts.provider = Some("mock".to_string());
    opts.max_steps = 12;
    opts.mcp = false;
    let mut app = App::new(opts);
    app.catalog = models::demo_catalog();
    app.model = "mimo-v2.6-flash".to_string();

    app.push(Entry::new(
        Kind::Task,
        "",
        "write notatka.txt :: plan przebiegu; read notatka.txt",
    ));
    let mut first = Entry::new(
        Kind::Thought,
        "",
        "Zapisuję plik, potem go odczytam, żeby sprawdzić treść.",
    )
    .step(1);
    first.latency = Some(Duration::from_millis(1200));
    app.push(first);
    let mut call = Entry::new(Kind::Call, "Writing", "notatka.txt")
        .detail("+38 B")
        .step(1);
    call.tool = "fs.write".to_string();
    call.running = false;
    app.push(call);
    let mut done = Entry::new(Kind::Ok, "Wrote", "notatka.txt")
        .detail("+38 B")
        .step(1);
    done.tool = "fs.write".to_string();
    app.push(done);

    let mut live = Entry::new(Kind::Thought, "", STREAM_SAMPLE).step(2);
    live.live = true;
    app.push(live);

    app.running = true;
    app.turn_started = Some(Instant::now());
    app.turn_chars = 1840;
    app.steps = 2;
    app.tool_calls = 1;
    app.input.clear();
    app.cursor = 0;
    app.show_scrollback_start();
    app
}

/// Ekran startowy do zrzutu: lista prawdziwych sesji z `.agent/sessions`
/// bieżącego workspace'u (nic nie tworzy, nic nie kasuje).
pub fn list_app(workspace: PathBuf) -> App {
    let mut opts = Options::new(workspace);
    opts.start = Start::Sessions;
    App::new(opts)
}

/// Ekran powitalny do zrzutu: prawdziwa ścieżka workspace'u, model i tryb
/// zgód — te same wartości, które zobaczy użytkownik.
pub fn welcome_app(workspace: PathBuf) -> App {
    let mut opts = Options::new(workspace);
    opts.start = Start::Welcome;
    App::new(opts)
}

/// Modal „Resume session" do zrzutu: prawdziwe transkrypty z `.agent/sessions`.
pub fn resume_app(workspace: PathBuf) -> App {
    let mut opts = Options::new(workspace);
    opts.start = Start::Resume;
    App::new(opts)
}

/// Ekran do zrzutu: prawdziwy przebieg pętli (mock) w osobnym workspace,
/// żeby obrazek pokazywał realne eventy, a nie atrapę.
pub fn demo(workspace: PathBuf) -> App {
    demo_with(workspace, false)
}

/// Ten sam przebieg, ale z opcjonalnymi skillami **przed** startem — wtedy
/// licznik `skille` w podsumowaniu i w statusie mówi to samo.
pub fn demo_with(workspace: PathBuf, skills: bool) -> App {
    let _ = std::fs::remove_dir_all(&workspace);
    let _ = std::fs::create_dir_all(&workspace);
    // Ścieżka bezwzględna: górny pasek skraca ją do `~/…` jak w zrzucie.
    let workspace = std::fs::canonicalize(&workspace).unwrap_or(workspace);
    if skills {
        seed_shot_skills(&workspace);
    }
    let mut opts = Options::new(workspace);
    opts.provider = Some("mock".to_string());
    opts.max_steps = 12;
    // Zrzut nie może zależeć od podprocesu perimetru — te same eventy,
    // narzędzia wprocesowe.
    opts.mcp = false;
    let mut app = App::new(opts);
    // Zrzut pokazuje licznik z `limit.context` i picker modeli; lista jest
    // statyczna, bo w trybie offline nie ma providera, którego można spytać.
    app.catalog = models::demo_catalog();
    app.model = "mimo-v2.6-flash".to_string();

    let (tx, rx) = mpsc::channel();
    context::set_event_sink(Some(tx));
    let cfg = app.config();
    let report = agent::run(DEMO_TASK, &cfg);
    context::set_event_sink(None);
    let mut events = Vec::new();
    while let Ok(value) = rx.try_recv() {
        events.push(value);
    }
    for value in events {
        app.handle_event(value);
    }
    app.running = false;
    app.turn_started = None;
    app.status = Some(report.status.clone());
    app.steps = report.steps;
    app.tool_calls = report.tool_calls;
    app.denied = report.denied;
    app.drift = report.drift;
    app.session = report
        .session
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string());
    app.input.clear();
    app.cursor = 0;
    app
}

/// Modal `/model` do zrzutu (`--screenshot-model`): demo z otwartym pickerem
/// na aktywnym modelu.
pub fn model_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.open_picker();
    app
}

/// Podpowiedzi `/` do zrzutu (`--screenshot-hints`): composer z `/mo`,
/// lista pod nim, zaznaczony `/model`.
pub fn hints_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.input = "/mo".to_string();
    app.cursor = 3;
    app.hints_hidden = false;
    app.hints_sel = 0;
    app.show_scrollback_start();
    app
}

/// Modal `/settings` do zrzutu (`--screenshot-settings`): demo z otwartym
/// modalem na pierwszym wierszu (motyw).
pub fn settings_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.open_settings();
    app.show_scrollback_start();
    app
}

/// Dwa skille do zrzutów (i tylko do nich): prawdziwe pliki `SKILL.md`
/// w `<workspace>/skills/`, więc modal czyta je tak samo jak u użytkownika.
pub fn seed_shot_skills(workspace: &std::path::Path) {
    let raport = workspace.join("skills").join("raport-dzienny");
    let notatki = workspace.join("skills").join("notatki");
    let _ = std::fs::create_dir_all(&raport);
    let _ = std::fs::create_dir_all(&notatki);
    let _ = std::fs::write(
        raport.join("SKILL.md"),
        "---\nname: raport-dzienny\ndescription: dzienny raport z postepu prac\nwhen: gdy trzeba podsumowac dzien albo tydzien\n---\n\
# Raport dzienny\n\
\n\
1. Zbierz wykonane kroki z transkryptu sesji.\n\
2. Podaj trzy liczby: kroki, narzedzia, odmowy.\n\
3. Napisz, co zostalo otwarte i kto to zamknie.\n\
4. Nie dodawaj opinii, ktorych nie ma w danych.\n\
\n\
Format: naglowek z data, lista zadan, lista otwartych rzeczy, jedna linia\n\
ryzyka. Kazda liczba z narzedzia, nie z pamieci.\n",
    );
    let _ = std::fs::write(
        notatki.join("SKILL.md"),
        "---\nname: notatki\ndescription: zapisuje notatki i decyzje do pliku\n---\n\
# Notatki\n\
\n\
Kazda notatka to trzy linie: decyzja, powod, data.\n\
Plik: notatki.md w workspace, dopisuj na koncu.\n",
    );
}

/// Modal `/skills` do zrzutu (`--screenshot-skills`): demo z dwoma skillami
/// w workspace i otwartą listą na pierwszym wierszu.
pub fn skills_shot(workspace: PathBuf) -> App {
    let mut app = demo_with(workspace, true);
    // Jeden skill już w kontekście — modal pokazuje oba znaczniki: zaznaczenie
    // (●) i „✓ w kontekście".
    app.run_skill("/skill notatki");
    app.open_skills();
    app.show_scrollback_start();
    app
}

/// Scrollback po `/skill` (`--screenshot-skill`): potwierdzenie wczytania
/// treści do kontekstu, na końcu przebiegu.
pub fn skill_shot(workspace: PathBuf) -> App {
    let mut app = demo_with(workspace, true);
    app.run_skill("/skill raport-dzienny");
    // Bez toasta: w kadrze ma być pasek statusu (`skille 2`), a potwierdzenie
    // wczytania stoi i tak w scrollbacku.
    app.toast = None;
    app
}

/// Pytanie trybu `ask` do zrzutu (`--screenshot-ask`): composer z prośbą
/// o zgodę na `fs.write nota.txt` (ten sam tekst, który zobaczy użytkownik).
pub fn ask_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.settings.approval = Approval::Ask;
    app.mode = "ask".to_string();
    app.question = Some(agent::ApprovalRequest {
        step: 2,
        tool: "fs.write".to_string(),
        arg: "nota.txt".to_string(),
    });
    app.show_scrollback_start();
    app
}

/// Motyw `jasny` do zrzutu (`--screenshot-light`): ten sam przebieg co `demo`,
/// tylko paleta jasna.
pub fn light_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.settings.theme = Theme::Light;
    app.show_scrollback_start();
    app
}

/// Modal szukania (`--screenshot-search`): demo i fraza „notatka" — trzy
/// wiersze trafień z numerami i licznikiem.
pub fn search_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.search_width.set(119);
    app.search_view.set(30);
    app.open_search();
    for ch in "notatka".chars() {
        app.on_key_search(
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE),
            false,
        );
    }
    app.toast = None;
    app
}

/// Scrollback po `/copy` (`--screenshot-copy`): potwierdzenie `skopiowano N
/// znakow (OSC 52)` pod przebiegiem.
pub fn copy_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.copy_block(false);
    app.toast = None;
    app
}

/// Scrollback po `/export` (`--screenshot-export`): potwierdzenie zapisu pliku
/// z liczbą kroków.
pub fn export_shot(workspace: PathBuf) -> App {
    let mut app = demo(workspace);
    app.export(None);
    app.toast = None;
    app
}

/// Testy współdzielą jeden globalny sink eventów, więc muszą iść po kolei.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp_ws(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "arndtai-tui-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn wait_done(app: &mut App) -> bool {
        for _ in 0..600 {
            app.poll();
            if !app.running {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn buffer_text(buffer: &Buffer, cols: u16, rows: u16) -> String {
        let mut out = String::new();
        for y in 0..rows {
            for x in 0..cols {
                if let Some(cell) = buffer.cell((x, y)) {
                    out.push_str(cell.symbol());
                }
            }
            out.push('\n');
        }
        out
    }

    fn render(app: &App, cols: u16, rows: u16) -> String {
        let backend = ratatui::backend::TestBackend::new(cols, rows);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        buffer_text(terminal.backend().buffer(), cols, rows)
    }

    /// Ten sam kadr co `render`, ale z kolorami — motyw sprawdza się na
    /// komórkach bufora, nie na tekście.
    fn render_buffer(app: &App, cols: u16, rows: u16) -> Buffer {
        let backend = ratatui::backend::TestBackend::new(cols, rows);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// `◆ Thought for 2.0s` tylko wtedy, gdy czas naprawdę zmierzono i jest
    /// dłuższy niż `THOUGHT_MIN`; inaczej sama etykieta `◆ Thought`.
    #[test]
    fn thought_line_shows_the_measured_time() {
        let _guard = test_guard();
        let ws = temp_ws("thought");
        let mut app = App::new(Options::new(ws.clone()));
        let mut quick = Entry::new(Kind::Thought, "", "krotka mysl");
        quick.latency = Some(Duration::from_millis(40));
        let mut slow = Entry::new(Kind::Thought, "", "dluga mysl");
        slow.latency = Some(Duration::from_millis(2000));
        app.entries.push(quick);
        app.entries.push(slow);

        let rows = app.rows(80);
        let lines: Vec<String> = rows
            .iter()
            .map(|row| row.spans.iter().map(|s| s.content.to_string()).collect::<String>())
            .collect();
        let diamond = format!("{} Thought", BULLET);
        assert!(
            lines.contains(&diamond),
            "krotka mysl bez czasu: {lines:?}"
        );
        assert!(
            lines.contains(&"◆ Thought for 2.0s".replace('◆', &BULLET.to_string())),
            "dluga mysl z czasem: {lines:?}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// `for Xs` liczy się od wysłania zadania do **pierwszej** treści, więc
    /// druga myśl tej samej tury nie dostaje drugiego, dłuższego czasu.
    #[test]
    fn thought_time_is_measured_once_per_turn() {
        let _guard = test_guard();
        let ws = temp_ws("thought-once");
        let mut app = App::new(Options::new(ws.clone()));
        app.turn_started = Some(Instant::now() - Duration::from_millis(2000));
        app.turn_first_thought = true;
        app.handle_event(serde_json::json!({
            "at": 1, "kind": "task", "data": { "text": "zadanie" }
        }));
        app.handle_event(serde_json::json!({
            "at": 1, "kind": "thought", "data": { "step": 1, "thought": "pierwsza" }
        }));
        app.handle_event(serde_json::json!({
            "at": 1, "kind": "thought", "data": { "step": 2, "thought": "druga" }
        }));
        assert!(app.entries[1].latency.is_some(), "pierwsza mysl ma czas");
        assert!(
            app.entries[2].latency.is_none(),
            "kolejna mysl nie dostaje czasu od wyslania"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Tekst asystenta: nagłówki pogrubione, listy `•`, reguły `——`, akapity
    /// rozdzielone jednym pustym wierszem (runda 18: robi to `markdown.rs`).
    #[test]
    fn assistant_text_renders_headings_bullets_and_rules() {
        let source = "## Plan\n\nAkapit **pogrubiony** i zwykly.\n\n- pierwszy\n- drugi\n\n---";
        let rows = markdown_rows(&markdown::Cache::new(), source, 40);
        let lines: Vec<String> = rows
            .iter()
            .map(|row| row.spans.iter().map(|s| s.content.to_string()).collect::<String>())
            .collect();
        assert_eq!(
            lines,
            vec![
                "Plan".to_string(),
                String::new(),
                "Akapit pogrubiony i zwykly.".to_string(),
                String::new(),
                format!("{} pierwszy", markdown::BULLET),
                format!("{} drugi", markdown::BULLET),
                String::new(),
                markdown::RULE_TEXT.to_string(),
            ],
            "uklad akapitow i list"
        );
        // Naglowek i wytluszczenie w akapicie ida boldem.
        assert!(rows[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::BOLD));
        assert!(rows[2]
            .spans
            .iter()
            .any(|span| span.content == "pogrubiony"
                && span.style.add_modifier.contains(Modifier::BOLD)));
        // Zawijanie trzyma style na słowach, więc tekst wiersza sklejamy.
        assert!(lines[2].contains("zwykly."), "reszta akapitu bez stylu");
    }

    /// Markdown obowiązuje **tylko** w odpowiedzi asystenta: tekst użytkownika
    /// i wynik narzędzia zostają dosłowne (`**` i `#` nie znikają).
    #[test]
    fn markdown_is_parsed_only_in_assistant_blocks() {
        let _guard = test_guard();
        let ws = temp_ws("md-scope");
        let mut app = App::new(Options::new(ws.clone()));
        app.entries.push(Entry::new(
            Kind::Task,
            "",
            "- nie lista **nie bold**".to_string(),
        ));
        let mut thought = Entry::new(Kind::Thought, "", "## Naglowek\n\n**bold**".to_string());
        thought.step = 1;
        app.entries.push(thought);
        let mut done = Entry::new(Kind::Ok, "Read", "a.txt".to_string());
        done.body = Some("## nie naglowek\n\n**nie bold**".to_string());
        app.entries.push(done);

        let rows = app.rows(80);
        let lines: Vec<String> = rows
            .iter()
            .map(|row| row.spans.iter().map(|s| s.content.to_string()).collect())
            .collect();
        let joined = lines.join("\n");
        assert!(
            joined.contains("- nie lista **nie bold**"),
            "zadanie uzytkownika doslownie: {joined}"
        );
        assert!(
            joined.contains("## nie naglowek"),
            "wynik narzedzia doslownie: {joined}"
        );
        assert!(
            joined.contains("**nie bold**"),
            "wynik narzedzia zachowuje gwiazdki: {joined}"
        );
        assert!(
            lines.iter().any(|line| line == "Naglowek"),
            "naglowek asystenta bez hashy: {lines:?}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Model dopisuje dalszy ciąg tej samej myśli: blok rośnie w miejscu,
    /// zamiast dokładać nowy wpis i przewijać scrollback.
    #[test]
    fn a_streamed_answer_grows_in_place() {
        let _guard = test_guard();
        let ws = temp_ws("md-stream");
        let mut app = App::new(Options::new(ws.clone()));
        for text in ["## Plan", "## Plan\n\n- pierwszy", "## Plan\n\n- pierwszy\n- drugi"] {
            app.handle_event(serde_json::json!({
                "at": 1, "kind": "thought", "data": { "step": 1, "thought": text }
            }));
        }
        assert_eq!(app.entries.len(), 1, "jeden wpis, nie trzy");
        assert_eq!(app.entries[0].subject, "## Plan\n\n- pierwszy\n- drugi");
        let rows = app.rows(80);
        assert!(
            rows.len() >= 4,
            "blok ma naglowek i dwa punkty: {}",
            rows.len()
        );
        // Inny krok to już nowa myśl, nie dopisanie.
        app.handle_event(serde_json::json!({
            "at": 1, "kind": "thought", "data": { "step": 2, "thought": "cos nowego" }
        }));
        assert_eq!(app.entries.len(), 2);
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Runda 21: przyrosty strumienia tworzą jeden blok, który rośnie w miejscu;
    /// zdarzenie `thought` zamyka go, nie dokładając drugiego wpisu.
    #[test]
    fn streamed_reasoning_grows_one_live_block_and_closes_it() {
        let _guard = test_guard();
        let ws = temp_ws("stream-live");
        let mut app = App::new(Options::new(ws.clone()));
        app.turn_started = Some(Instant::now());
        for (i, piece) in ["Sprawdzam", " plik", " i plan"].iter().enumerate() {
            app.apply_delta(provider::Delta {
                step: 1,
                channel: provider::Channel::Reasoning,
                text: piece.to_string(),
            });
            assert_eq!(app.entries.len(), 1, "krok {}: jeden blok", i);
            assert!(app.entries[0].live, "blok rośnie na żywo");
        }
        assert_eq!(app.entries[0].subject, "Sprawdzam plik i plan");
        // Kanał `content` to JSON decyzji — nie trafia do scrollbacku.
        app.apply_delta(provider::Delta {
            step: 1,
            channel: provider::Channel::Content,
            text: "{\"thought\":\"x\"}".to_string(),
        });
        assert_eq!(app.entries.len(), 1);
        assert!(!app.entries[0].subject.contains("thought"));
        app.handle_event(serde_json::json!({
            "at": 1, "kind": "thought", "data": { "step": 1, "thought": "Sprawdzam plik i plan" }
        }));
        assert_eq!(app.entries.len(), 1, "zamknięcie w miejscu, bez duplikatu");
        assert!(!app.entries[0].live);
        // Kolejny krok dostaje własny bloczek.
        app.apply_delta(provider::Delta {
            step: 2,
            channel: provider::Channel::Reasoning,
            text: "Dalej".to_string(),
        });
        assert_eq!(app.entries.len(), 2);
        assert!(app.entries[1].live);
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Zrzut rundy 21: w trakcie strumienia widać znacznik i rosnący tekst.
    #[test]
    fn stream_screenshot_shows_the_live_thought_block() {
        let _guard = test_guard();
        let ws = temp_ws("stream-shot");
        let app = stream_shot(ws.clone());
        let screen = render(&app, 120, 38);
        assert!(screen.contains("streaming"), "brak znacznika: {}", screen);
        assert!(
            screen.contains("Sprawdzam plik i planuję"),
            "brak tekstu myśli: {}",
            screen
        );
        assert!(screen.contains("Responding"), "brak wiersza aktywności");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Oba kanały są niezależne: `poll` musi najpierw położyć pas zadania,
    /// potem dopisać przyrost, a na końcu domknąć blok zdarzeniem `thought`.
    #[test]
    fn poll_keeps_task_above_the_streamed_thought() {
        let _guard = test_guard();
        let ws = temp_ws("stream-order");
        let mut app = App::new(Options::new(ws.clone()));
        let (event_tx, event_rx) = mpsc::channel();
        let (delta_tx, delta_rx) = mpsc::channel();
        app.events = Some(event_rx);
        app.deltas = Some(delta_rx);
        app.running = true;
        app.turn_started = Some(Instant::now());
        event_tx
            .send(serde_json::json!({
                "at": 1, "kind": "task", "data": { "text": "zadanie" }
            }))
            .unwrap();
        delta_tx
            .send(provider::Delta {
                step: 1,
                channel: provider::Channel::Reasoning,
                text: "myśl".to_string(),
            })
            .unwrap();
        event_tx
            .send(serde_json::json!({
                "at": 1, "kind": "thought", "data": { "step": 1, "thought": "myśl" }
            }))
            .unwrap();
        app.poll();
        assert_eq!(app.entries.len(), 2, "pas zadania i jeden blok myśli");
        assert!(matches!(app.entries[0].kind, Kind::Task));
        assert!(matches!(app.entries[1].kind, Kind::Thought));
        assert_eq!(app.entries[1].subject, "myśl");
        assert!(!app.entries[1].live, "thought zamyka blok");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Zrzut rundy 18: przy 120 i przy 40 kolumnach ten sam ekran mieści
    /// nagłówek, listę, kod z etykietą, tabelę i cytat.
    #[test]
    fn the_markdown_screen_holds_together_at_forty_columns() {
        let _guard = test_guard();
        let ws = temp_ws("md-shot");
        let app = markdown_shot(ws.clone());
        for (cols, rows) in [(120u16, 38u16), (40, 38)] {
            let screen = render(&app, cols, rows);
            for expected in [
                "Plan przebiegu",
                "1. zapis pliku",
                "2. odczyt i sprawdzenie",
                "fn main()",
                " rust ",
                "Wynik",
                "poza workspace to odmowa",
            ] {
                assert!(
                    screen.contains(expected),
                    "{cols} kolumn: brak {:?} w ekranie",
                    expected
                );
            }
            // Siatka tabeli i ramka kodu są na ekranie.
            assert!(screen.contains('\u{2502}'), "{cols} kolumn: brak kresek");
            assert!(
                screen.lines().all(|line| line.chars().count() <= cols as usize),
                "{cols} kolumn: wiersz wyszedl za ekran"
            );
        }
        let narrow = render(&app, 40, 38);
        assert!(
            !narrow.contains('\u{2510}') && !narrow.contains('\u{2518}'),
            "przy 40 kolumnach ramka kodu bez prawej krawedzi"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Kolory podświetlania muszą dojechać do bufora ekranu — nie tylko do
    /// spanów renderera.
    #[test]
    fn code_colors_reach_the_screen() {
        let _guard = test_guard();
        let ws = temp_ws("md-colors");
        let app = markdown_shot(ws.clone());
        let buffer = render_buffer(&app, 120, 38);
        // Pozycja igły na ekranie: (kolumna w znakach, wiersz).
        let locate = |needle: &str| -> (u16, u16) {
            for y in 0..38u16 {
                let line: String = (0..120)
                    .filter_map(|x| buffer.cell((x, y)).map(|c| c.symbol().to_string()))
                    .collect();
                if let Some(at) = line.find(needle) {
                    // Kolumny liczymy w znakach: `│` w ramce to trzy bajty.
                    return (line[..at].chars().count() as u16, y);
                }
            }
            panic!("brak {needle:?} na ekranie");
        };
        let (x, y) = locate("fn main()");
        assert_eq!(
            buffer.cell((x, y)).unwrap().fg,
            MAGENTA,
            "slowo kluczowe 'fn' w kolorze palety"
        );
        let (three, ty) = locate("let kroki = 3");
        assert_eq!(
            buffer.cell((three + 12, ty)).unwrap().fg,
            ORANGE,
            "liczba '3' w kolorze palety"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Runda 13: po `FAILED_ERROR` (np. perimetr nie wystartowal, bo binarke
    /// podmieniono) tekst zadania wraca do composera — bez przepisywania.
    #[test]
    fn failed_turn_gives_the_task_back_to_the_composer() {
        let _guard = test_guard();
        let ws = temp_ws("restore");
        let mut opts = Options::new(ws.clone());
        opts.provider = Some("mock".to_string());
        opts.mcp = false;
        let mut app = App::new(opts);

        let task = "write a.txt :: alfa";
        app.input = task.to_string();
        app.cursor = app.input.chars().count();
        app.submit();
        assert!(app.input.is_empty(), "wysylka czysci composer");
        assert_eq!(app.last_task.as_deref(), Some(task));
        assert!(wait_done(&mut app), "petla nie skonczyla sie w limicie");

        app.finish_report(agent::Report {
            status: "FAILED_ERROR".to_string(),
            steps: 0,
            tool_calls: 0,
            denied: 0,
            drift: 0,
            reason: "perimeter MCP nie wystartowal".to_string(),
            session: ws.join(".agent/sessions/s1.jsonl"),
        });

        assert_eq!(app.input, task, "tekst musi wrocic do composera");
        assert_eq!(
            app.cursor,
            app.input.chars().count(),
            "kursor na koncu odzyskanego tekstu"
        );
        assert_eq!(app.status.as_deref(), Some("FAILED_ERROR"));

        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Udana tura nie dokleja starego zadania z powrotem — composer zostaje pusty.
    #[test]
    fn finished_turn_leaves_the_composer_empty() {
        let _guard = test_guard();
        let ws = temp_ws("norestore");
        let mut opts = Options::new(ws.clone());
        opts.provider = Some("mock".to_string());
        opts.mcp = false;
        let mut app = App::new(opts);

        app.input = "write a.txt :: alfa".to_string();
        app.cursor = app.input.chars().count();
        app.submit();
        assert!(wait_done(&mut app), "petla nie skonczyla sie w limicie");

        assert!(app.input.is_empty(), "po DONE composer zostaje pusty");
        assert_eq!(app.status.as_deref(), Some("DONE"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Tekst wpisany juz po wyslaniu jest nowszy — nie nadpisujemy go starym.
    #[test]
    fn failed_turn_keeps_newer_typing() {
        let _guard = test_guard();
        let ws = temp_ws("newer");
        let mut opts = Options::new(ws.clone());
        opts.provider = Some("mock".to_string());
        opts.mcp = false;
        let mut app = App::new(opts);

        app.input = "stare zadanie".to_string();
        app.cursor = app.input.chars().count();
        app.submit();
        app.input = "nowe".to_string();
        app.cursor = 4;

        app.finish_report(agent::Report {
            status: "FAILED_ERROR".to_string(),
            steps: 0,
            tool_calls: 0,
            denied: 0,
            drift: 0,
            reason: "x".to_string(),
            session: ws.join(".agent/sessions/s1.jsonl"),
        });

        assert_eq!(app.input, "nowe");
        assert_eq!(app.cursor, 4);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn typing_a_task_streams_tool_call_and_result() {
        let _guard = test_guard();
        let ws = temp_ws("stream");
        let mut opts = Options::new(ws.clone());
        opts.provider = Some("mock".to_string());
        opts.mcp = false;
        let mut app = App::new(opts);

        for ch in "write tui.txt :: alfa; read tui.txt".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.running, "Enter musi wystartowac petle");
        assert!(wait_done(&mut app), "petla nie skonczyla sie w limicie");

        let tools: Vec<&Entry> = app
            .entries
            .iter()
            .filter(|e| matches!(e.kind, Kind::Ok | Kind::Fail))
            .collect();
        assert_eq!(tools.len(), 2, "dwa wywolania narzedzi w scrollbacku");
        assert!(
            app.entries.iter().all(|e| !e.running),
            "wynik zamyka wpis wywolania w miejscu"
        );
        assert!(
            app.entries
                .iter()
                .any(|e| e.kind == Kind::Ok && e.subject.contains("tui.txt")),
            "wynik fs.read musi byc widoczny"
        );
        assert_eq!(app.status.as_deref(), Some("DONE"));
        assert_eq!(app.tool_calls, 2);
        assert_eq!(std::fs::read_to_string(ws.join("tui.txt")).unwrap(), "alfa");
        let transcript = std::fs::read_dir(ws.join(".agent/sessions"))
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0);
        assert_eq!(transcript, 1, "sesja zapisana jak dotad (JSONL)");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn denied_call_is_visible_and_counted() {
        let _guard = test_guard();
        let ws = temp_ws("denied");
        let mut opts = Options::new(ws.clone());
        opts.provider = Some("mock".to_string());
        opts.mcp = false;
        let mut app = App::new(opts);
        app.input = "write ../poza.txt :: x".to_string();
        app.cursor = app.input.chars().count();
        app.submit();
        assert!(wait_done(&mut app));
        assert!(app.entries.iter().any(|e| e.kind == Kind::Denied));
        assert_eq!(app.denied, 1);
        assert!(!ws.parent().unwrap().join("poza.txt").exists());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn layout_keeps_every_pane_inside_the_screen() {
        let _guard = test_guard();
        for rows in 6u16..60 {
            for cols in [24u16, 40, 80, 120, 200] {
                let area = Rect::new(0, 0, cols, rows);
                for running in [false, true] {
                    let screen = layout(area, running, 1);
                    let panes = [
                        screen.top,
                        screen.scrollback,
                        screen.turn,
                        screen.prompt,
                        screen.footer,
                    ];
                    for pane in panes {
                        if pane.height == 0 {
                            continue;
                        }
                        assert!(
                            pane.x + pane.width <= area.x + area.width
                                && pane.y + pane.height <= area.y + area.height,
                            "pane {:?} wychodzi poza ekran {:?} (wiersze {})",
                            pane,
                            area,
                            rows
                        );
                    }
                    assert!(
                        screen.footer.y >= screen.prompt.y + screen.prompt.height,
                        "stopka musi byc pod composerem (wiersze {})",
                        rows
                    );
                    if rows >= 22 {
                        assert!(
                            screen.scrollback.height >= SCROLLBACK_MIN_ROWS,
                            "scrollback ma minimum {} wierszy, jest {} (wiersze {})",
                            SCROLLBACK_MIN_ROWS,
                            screen.scrollback.height,
                            rows
                        );
                    }
                }
            }
        }
    }

    /// Górny pasek ze zrzutu: ścieżka po lewej, licznik kontekstu po prawej,
    /// nic w środku (bez modelu i bez sesji).
    #[test]
    fn top_bar_has_path_on_the_left_and_context_counter_on_the_right() {
        let _guard = test_guard();
        let app = demo(temp_ws("topbar").join("agent"));
        let text = render(&app, 120, 34);
        let line = text
            .lines()
            .find(|l| !l.trim().is_empty())
            .expect("górny pasek jest pierwszym niepustym wierszem");
        let trimmed = line.trim();
        assert!(
            trimmed.starts_with('~') || trimmed.starts_with('/'),
            "ścieżka zaczyna wiersz: {line:?}"
        );
        assert!(
            line.contains(" / 1.0M"),
            "licznik kontekstu po prawej: {line:?}"
        );
        assert!(
            line.trim_end().ends_with("1.0M"),
            "licznik domyka wiersz: {line:?}"
        );
        // Offline nie ma `usage`, więc licznik jest szacunkiem z `~`.
        let used = line.split(" / ").next().unwrap_or("");
        let token = used.split_whitespace().last().unwrap_or("");
        assert!(
            token.starts_with('~') && token.len() > 1,
            "szacunek ma tylde: {line:?}"
        );
        assert!(!line.contains("sesja"), "bez sesji w środku: {line:?}");
        assert!(
            !line.contains("ArndtAi"),
            "bez nazwy/modelu w pasku: {line:?}"
        );
        let counter = line.find(" / 1.0M").unwrap();
        assert!(
            counter > line.len() / 2,
            "licznik przy prawej krawędzi: {line:?}"
        );
    }

    /// Przy wąskim oknie ścieżka traci początek, a licznik zostaje cały.
    #[test]
    fn top_bar_trims_the_path_before_the_counter() {
        assert_eq!(trim_left("~/projects/ArndtAi", 40), "~/projects/ArndtAi");
        assert_eq!(trim_left("~/projects/ArndtAi", 5), "\u{2026}dtAi");
        assert_eq!(trim_left("~/projects/ArndtAi", 1), "\u{2026}");
        assert_eq!(trim_left("~/x", 0), "");

        let _guard = test_guard();
        let app = demo(temp_ws("topbar-narrow"));
        let text = render(&app, 40, 14);
        let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        assert!(
            line.trim_end().ends_with("1.0M"),
            "licznik zostaje przy 40 kolumnach: {line:?}"
        );
        assert!(
            line.chars().filter(|c| !c.is_whitespace()).count() > 0,
            "pasek nie może być pusty"
        );
    }

    /// Układ ze zrzutu `readme-tui.png`: pas zadania, `◆ Thought`, tekst pełną
    /// szerokością, composer z etykietą modelu, stopka z ` │ `.
    #[test]
    fn conversation_screen_follows_the_readme_screenshot() {
        let _guard = test_guard();
        let ws = temp_ws("render");
        let app = demo(ws.clone());
        let text = render(&app, 110, 34);
        assert!(text.contains('\u{276F}'), "prefiks ❯ w pasie zadania: {text}");
        assert!(
            text.contains('\u{25C6}') && text.contains("Thought"),
            "naglowek mysli z rombem: {text}"
        );
        assert!(text.contains("Read "), "naglowek narzedzia: {text}");
        assert!(text.contains("Odmowa"), "odmowa widoczna: {text}");
        assert!(text.contains("DONE"), "status koncowy: {text}");
        assert!(text.contains('\u{256D}'), "ramka promptu: {text}");
        assert!(
            text.contains("Enter:send"),
            "stopka bez spacji przed dwukropkiem: {text}"
        );
        assert!(
            text.contains(" \u{2502} Ctrl+L:clear"),
            "separator stopki ` │ `: {text}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn scroll_stops_at_both_ends() {
        let _guard = test_guard();
        let app = demo(temp_ws("scroll"));
        let mut app = app;
        let top = render(&app, 100, 24);
        app.scroll = usize::MAX / 2;
        let scrolled = render(&app, 100, 24);
        assert_ne!(top, scrolled, "PgUp musi zmienic widok");
        app.scroll = 0;
        assert_eq!(render(&app, 100, 24), top, "PgDn wraca do konca");
    }

    #[test]
    fn resize_does_not_panic_and_keeps_the_prompt() {
        let _guard = test_guard();
        let app = demo(temp_ws("resize"));
        for (cols, rows) in [(40u16, 12u16), (60, 16), (80, 20), (120, 38)] {
            let text = render(&app, cols, rows);
            assert!(!text.trim().is_empty());
        }
    }

    /// W trakcie tury nad composerem stoi `⠋ Responding… Ns` po lewej
    /// i `Ns ↓N.NNk` po prawej (znaki zdarzeń, nie tokeny).
    #[test]
    fn running_turn_shows_responding_row_with_stream_counter() {
        let _guard = test_guard();
        let ws = temp_ws("pulse");
        let mut opts = Options::new(ws.clone());
        opts.provider = Some("mock".to_string());
        opts.mcp = false;
        let mut app = App::new(opts);
        let mut call = Entry::new(Kind::Call, "Reading", "a.txt").step(1);
        call.running = true;
        app.entries.push(call);
        app.running = true;
        app.turn_started = Some(Instant::now());
        app.turn_chars = 9450;

        let text = render(&app, 100, 24);
        assert!(text.contains("Responding"), "wiersz aktywnosci: {text}");
        assert!(text.contains("\u{2193}9.45k"), "licznik strumienia: {text}");
        assert!(text.contains("Reading a.txt"), "naglowek wywolania: {text}");
        // Runda 16: `skille N` wydłuża status, więc przy 100 kolumnach stopka
        // zwija ostatnią podpowiedź — status zostaje.
        assert!(text.contains("skille 0"), "licznik skilli w statusie: {text}");
        let wide = render(&app, 120, 24);
        assert!(
            wide.contains("Ctrl+C:quit"),
            "stopka w trakcie tury: {wide}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    // ── Ekran startowy: lista sesji i nawigacja ──────────────────────────

    fn write_transcript(ws: &Path, id: &str, lines: &[Value]) {
        let dir = ws.join(".agent/sessions");
        let _ = std::fs::create_dir_all(&dir);
        let body: String = lines
            .iter()
            .map(|v| format!("{}\n", v))
            .collect::<Vec<_>>()
            .join("");
        std::fs::write(dir.join(format!("{}.jsonl", id)), body).unwrap();
    }

    fn list_ws(tag: &str) -> PathBuf {
        let ws = temp_ws(tag);
        let now = sessions::now_secs();
        write_transcript(
            &ws,
            "700",
            &[
                serde_json::json!({"at": now - 120, "kind": "task", "data": {"text": "write alfa.txt :: alfa"}}),
                serde_json::json!({"at": now - 100, "kind": "thought", "data": {"step": 1, "thought": "krok 1"}}),
                serde_json::json!({"at": now - 90, "kind": "tool_call", "data": {"step": 1, "tool": "fs.write", "args": {"path": "alfa.txt"}}}),
                serde_json::json!({"at": now - 80, "kind": "tool_result", "data": {"step": 1, "tool": "fs.write", "ok": true, "bytes": 4}}),
                serde_json::json!({"at": now - 70, "kind": "summary", "data": {"status": "DONE", "steps": 1, "tool_calls": 1, "denied": 0}}),
            ],
        );
        write_transcript(
            &ws,
            "800",
            &[
                serde_json::json!({"at": now - 60, "kind": "task", "data": {"text": "read brak.txt"}}),
                serde_json::json!({"at": now - 50, "kind": "error", "data": {"message": "brak pliku"}}),
                serde_json::json!({"at": now - 40, "kind": "summary", "data": {"status": "FAILED_ERROR", "steps": 1, "tool_calls": 1, "denied": 0}}),
            ],
        );
        ws
    }

    fn list_app_for(ws: &Path) -> App {
        let mut opts = Options::new(ws.to_path_buf());
        opts.start = Start::Sessions;
        App::new(opts)
    }

    #[test]
    fn session_list_renders_real_rows_and_group_headers() {
        let _guard = test_guard();
        let ws = list_ws("list-render");
        let app = list_app_for(&ws);
        assert_eq!(app.view, View::Sessions);
        assert_eq!(app.visible_sessions().len(), 2, "dwa prawdziwe transkrypty");

        let text = render(&app, 120, 38);
        assert!(text.contains("Gotowe"), "grupa DONE: {text}");
        assert!(text.contains("Blad"), "grupa FAILED: {text}");
        assert!(text.contains("write alfa.txt"), "tytul z zadania: {text}");
        assert!(text.contains("read brak.txt"), "drugi wiersz: {text}");
        assert!(text.contains('\u{25BE}'), "naglowek grupy ma disclosure: {text}");
        assert!(text.contains("Nowa sesja"), "wiersz akcji: {text}");
        assert!(text.contains("Filtr: wszystkie"), "filtr w naglowku: {text}");
        assert!(text.contains("Wrote alfa.txt"), "druga linia z aktywnoscia: {text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// 40 kolumn to `MIN_DASHBOARD_WIDTH`: wiersz traci badge i subtitle,
    /// ale naglowek, filtr, tytul i grupy zostaja w kadrze. Krotszy terminal
    /// przewija liste, nie rozsypuje ukladu.
    #[test]
    fn session_list_collapses_at_narrow_width() {
        let _guard = test_guard();
        for (cols, rows) in [(40u16, 20u16), (40, 12), (24, 10)] {
            let ws = list_ws("list-narrow");
            let app = list_app_for(&ws);
            let text = render(&app, cols, rows);
            assert!(!text.trim().is_empty(), "{cols}x{rows}: pusty ekran");
            for line in text.lines() {
                assert!(
                    line.chars().count() <= cols as usize,
                    "{cols} kolumn: linia wychodzi za ekran: {line}"
                );
            }
            // Filtr zostaje nawet przy 24 kolumnach (w formie skrótu `/ …`).
            assert!(
                text.contains("Filtr") || text.contains("wszystkie"),
                "{cols}x{rows}: brak filtra w naglowku: {text}"
            );
            if cols >= 40 && rows >= 20 {
                assert!(
                    text.contains("Gotowe") || text.contains("Blad"),
                    "{cols}x{rows}: brak naglowka grupy: {text}"
                );
            }
            let _ = std::fs::remove_dir_all(&ws);
        }
    }

    /// Skroty `n` i `/` nie zjadaja liter w polu nowego zadania: kiedy pole
    /// ma juz tekst, kazdy znak jest trescia (skrot dziala tylko na pustym).
    #[test]
    fn dispatch_field_text_wins_over_shortcut_letters() {
        let _guard = test_guard();
        let ws = list_ws("list-typing");
        let mut app = list_app_for(&ws);
        for ch in "abn/x".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        assert_eq!(app.input, "abn/x");
        assert_eq!(app.view, View::Sessions, "pole tekstowe nie otwiera sesji");
        assert_eq!(app.filter, Filter::All, "`/` w tekscie nie zmienia filtra");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn list_navigation_moves_selection_and_enter_opens_session() {
        let _guard = test_guard();
        let ws = list_ws("list-nav");
        let mut app = list_app_for(&ws);
        assert_eq!(app.selected, 0);
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.selected, 1, "strzalka w dol przesuwa zaznaczenie");
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.selected, 0);
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(app.selected, 0, "górna granica nie ucieka");

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.view, View::Session, "Enter wchodzi w sesje");
        assert_eq!(app.session.as_deref(), Some("700"));
        assert!(
            app.entries.iter().any(|e| e.kind == Kind::Task),
            "transkrypt wraca do scrollbacku"
        );
        assert_eq!(app.status.as_deref(), Some("DONE"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn welcome_screen_opens_the_resume_modal_on_real_sessions() {
        let _guard = test_guard();
        let ws = list_ws("welcome");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Welcome;
        let mut app = App::new(opts);
        assert_eq!(app.view, View::Welcome, "`ardntai tui` bez zadania = powitanie");

        let text = render(&app, 83, 38);
        assert!(text.contains("Resume session"), "menu ze zrzutu: {text}");
        assert!(text.contains("ctrl+r"));
        assert!(text.contains("Changelog"));
        assert!(text.contains("always-approve"), "tryb z konfiguracji: {text}");
        assert!(
            text.contains(concat!("ArndtAi ", env!("CARGO_PKG_VERSION"))),
            "wersja z Cargo: {text}"
        );
        assert!(text.contains('\u{256D}'), "zaokraglona ramka composera: {text}");

        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert_eq!(app.view, View::Resume, "ctrl+r otwiera modal");
        assert_eq!(app.resume_sessions().len(), 2, "modal czyta prawdziwe transkrypty");

        let text = render(&app, 85, 38);
        assert!(text.contains("Resume session"));
        assert!(text.contains("2 sessions"));
        assert!(text.contains("write alfa.txt"), "tytul sesji w modalu: {text}");
        assert!(text.contains("Esc:back"), "stopka z klawiszami: {text}");

        app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.filter, Filter::Failed, "`f` przelacza filtr");
        assert_eq!(app.resume_sessions().len(), 1, "filtr bledow zweza liste");

        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, View::Welcome, "Esc zamyka modal do powitania");
        assert!(!app.quit);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn resume_search_narrows_titles_and_enter_opens_the_session() {
        let _guard = test_guard();
        let ws = list_ws("resume-search");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Welcome;
        let mut app = App::new(opts);
        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));

        app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert!(app.searching);
        for ch in "alfa".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        assert_eq!(app.query, "alfa");
        assert_eq!(app.resume_sessions().len(), 1, "szukanie po tytule");
        assert_eq!(app.resume_sessions()[0].id, "700");

        // `f` w trybie szukania jest literą, nie filtrem.
        app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::NONE));
        assert_eq!(app.query, "alfaf");
        assert_eq!(app.resume_sessions().len(), 0, "brak trafien");
        let text = render(&app, 85, 38);
        assert!(text.contains("No matches"), "{text}");

        app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(app.resume_sessions().len(), 1);

        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.view, View::Session, "Enter otwiera sesje z modala");
        assert_eq!(app.session.as_deref(), Some("700"));
        assert!(app.entries.iter().any(|e| e.kind == Kind::Task));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn welcome_and_resume_survive_a_narrow_terminal() {
        let _guard = test_guard();
        let ws = list_ws("welcome-narrow");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Welcome;
        let mut app = App::new(opts);
        for (cols, rows) in [(40u16, 20u16), (24, 8)] {
            let text = render(&app, cols, rows);
            for line in text.lines() {
                assert!(
                    line.chars().count() <= cols as usize,
                    "{cols} kolumn: linia wychodzi za ekran: {line}"
                );
            }
            app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
            let text = render(&app, cols, rows);
            assert!(text.contains("Resume session"), "{cols}x{rows}: {text}");
            assert!(!text.trim().is_empty());
            app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        }
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn slash_cycles_filter_and_esc_quits_from_list() {
        let _guard = test_guard();
        let ws = list_ws("list-filter");
        let mut app = list_app_for(&ws);
        app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.filter, Filter::Failed);
        assert_eq!(app.visible_sessions().len(), 1, "filtr bledow zawęża liste");
        let text = render(&app, 120, 38);
        assert!(text.contains("Filtr: bledy"), "etykieta filtra: {text}");
        assert!(text.contains("read brak.txt"));
        assert!(!text.contains("write alfa.txt"), "DONE wypada z filtra: {text}");

        app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.filter, Filter::Running);
        assert_eq!(app.visible_sessions().len(), 0, "nic nie leci");

        app.on_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
        assert_eq!(app.filter, Filter::All);
        assert_eq!(app.visible_sessions().len(), 2);

        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.quit, "Esc na liscie konczy program");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn n_starts_a_fresh_session_from_the_list() {
        let _guard = test_guard();
        let ws = list_ws("list-new");
        let mut app = list_app_for(&ws);
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(app.view, View::Session);
        assert!(app.entries.is_empty(), "nowa sesja nie dziedziczy scrollbacku");
        assert!(app.session.is_none());
        assert!(app.input.is_empty());
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.view, View::Sessions, "Esc z sesji wraca do listy");
        assert!(!app.quit);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn composer_height_follows_text_and_keeps_scrollback_minimum() {
        let _guard = test_guard();
        let area = Rect::new(0, 0, 120, 40);
        let one = layout(area, false, 1).prompt.height;
        let three = layout(area, false, 3).prompt.height;
        let many = layout(area, false, 30).prompt.height;
        assert_eq!(one, PROMPT_HEIGHT, "vpad + 1 wiersz + info = 3");
        assert_eq!(three, 5, "trzy wiersze tekstu = 5");
        assert!(many > three, "composer rosnie z tekstem");
        assert!(many <= area.height / 2, "sufit: polowa ekranu");
        let screen = layout(area, false, 30);
        assert!(
            screen.scrollback.height >= SCROLLBACK_MIN_ROWS,
            "scrollback nie schodzi pod minimum: {}",
            screen.scrollback.height
        );
        // Niski terminal: composer nie zjada scrollbacku ani paska.
        let low = layout(Rect::new(0, 0, 40, 20), false, 30);
        assert!(low.prompt.height <= 10);
        assert!(low.scrollback.height >= SCROLLBACK_MIN_ROWS);
        assert!(low.prompt.y + low.prompt.height <= low.footer.y);
        let _ = grid_height_check();
    }

    fn grid_height_check() -> u16 {
        composer_grid("ala ma kota", 4).len() as u16
    }

    #[test]
    fn fs_write_draws_a_diff_block() {
        let _guard = test_guard();
        let ws = temp_ws("diff");
        let mut app = App::new(Options::new(ws.clone()));
        let mut done = Entry::new(Kind::Ok, "Wrote", "notatka.txt")
            .detail("+9 B")
            .step(1);
        done.content = Some("pierwsza\ndruga".to_string());
        app.entries.push(done);
        let rows = app.rows(80);
        let diff: Vec<&Row> = rows
            .iter()
            .filter(|row| row.bg == Some(GREEN_DARK))
            .collect();
        assert_eq!(diff.len(), 2, "kazda zapisana linia to wiersz diffu");
        let text: String = diff
            .iter()
            .flat_map(|row| row.spans.iter())
            .map(|span| span.content.to_string())
            .collect();
        assert!(text.contains("1"), "numer linii w rynnie: {text}");
        assert!(text.contains("pierwsza"));
        assert!(text.contains("druga"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn task_entry_uses_the_prompt_band_from_the_original() {
        let _guard = test_guard();
        let ws = temp_ws("task-band");
        let mut app = App::new(Options::new(ws.clone()));
        app.entries.push(Entry::new(
            Kind::Task,
            "",
            "write a.txt :: alfa; read a.txt".to_string(),
        ));
        let rows = app.rows(80);
        // Ze zrzutu: jeden wiersz tekstu + po jednym wierszu odstępu nad i pod.
        assert_eq!(rows.len(), 3, "pas zadania to trzy wiersze: odstep, tekst, odstep");
        assert!(rows[0].spans.is_empty() && rows[2].spans.is_empty());
        for row in &rows {
            // `prompt_band_color_for()` → `theme.bg_light` (#242424).
            assert_eq!(row.bg, Some(BG_LIGHT), "pas zadania bierze bg_light");
            assert_ne!(row.bg, Some(GREEN_DARK), "GREEN_DARK nalezy do diffu");
        }
        let row = &rows[1];
        let prefix = format!(" {}", PROMPT_ARROW);
        assert!(
            row.spans[0].content.starts_with(&prefix),
            "prefiks ❯ na poczatku: {:?}",
            row.spans[0].content
        );
        // `prompt_styles()` → prefiks `accent_user` (#c8c8c8), tekst `text_primary`.
        assert_eq!(row.spans[0].style.fg, Some(ACCENT_USER), "prefiks accent_user");
        assert_eq!(row.spans[1].style.fg, Some(FG), "tekst text_primary");
        assert_eq!(row.spans[1].style.bg, Some(BG_LIGHT), "tekst na pasie");
        // Godzina po prawej krawędzi pasa (`8:19 AM` w oryginale, u nas HH:MM).
        let (stamp, _) = row.right.as_ref().expect("znacznik czasu po prawej");
        assert_eq!(stamp.len(), 5, "HH:MM: {stamp}");
        assert_eq!(&stamp[2..3], ":");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// `gap_after: 1` — miedzy wpisami dokladnie jeden pusty wiersz, takze
    /// po bloku wieloliniowym (mysl z trescia).
    #[test]
    fn entries_are_separated_by_exactly_one_blank_row() {
        let _guard = test_guard();
        let ws = temp_ws("task-gap");
        let mut app = App::new(Options::new(ws.clone()));
        app.entries.push(Entry::new(Kind::Task, "", "zadanie".to_string()));
        let mut thought = Entry::new(Kind::Thought, "", "krok 1 z planu".to_string());
        thought.detail = "krok 1".to_string();
        app.entries.push(thought);
        let mut done = Entry::new(Kind::Ok, "Wrote", "a.txt".to_string());
        done.detail = "+4 B".to_string();
        done.content = Some("alfa".to_string());
        app.entries.push(done);
        let rows = app.rows(80);
        let blank = |row: &Row| row.spans.is_empty() && row.bg.is_none();
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut i = 0;
        while i < rows.len() {
            if blank(&rows[i]) {
                let start = i;
                while i < rows.len() && blank(&rows[i]) {
                    i += 1;
                }
                runs.push((start, i - start));
            } else {
                i += 1;
            }
        }
        assert_eq!(
            runs,
            vec![(3, 1), (5, 1), (7, 1), (9, 1)],
            "kazda przerwa ma dokladnie jeden wiersz: {runs:?}"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Panel peek stoi po prawej stronie listy i nie zabiera jej
    /// `MIN_DASHBOARD_WIDTH` kolumn; przy wezszym terminalu znika.
    #[test]
    fn peek_panel_keeps_the_list_at_min_dashboard_width() {
        let _guard = test_guard();
        for cols in [120u16, 100, 80, 76, 70, 68, 40, 24] {
            let area = Rect::new(0, 0, cols, 38);
            let panes = list_layout(area);
            if panes.peek.width > 0 {
                assert!(
                    panes.list.width >= MIN_DASHBOARD_WIDTH,
                    "{cols} kolumn: lista ma tylko {} kolumn",
                    panes.list.width
                );
                assert!(
                    panes.peek.x >= panes.list.x + panes.list.width + 1,
                    "{cols} kolumn: panel nachodzi na liste"
                );
                assert!(
                    panes.peek.height <= (u32::from(38u16) * 3 / 8) as u16,
                    "{cols} kolumn: peek wyzszy niz 3/8 ekranu"
                );
                assert!(panes.peek.height >= PEEK_MIN_BOX_LIVE_TAIL);
                assert_eq!(
                    panes.peek.y, panes.list.y,
                    "panel startuje razem z lista"
                );
            } else {
                assert_eq!(
                    panes.list.width,
                    area.width.saturating_sub(LIST_OUTER_HPAD * 2),
                    "{cols} kolumn: bez peeku lista bierze cala szerokosc"
                );
            }
            if cols <= 68 {
                assert_eq!(panes.peek.width, 0, "{cols} kolumn: peek musi zniknac");
            }
        }
    }

    /// Panel rysuje ostatnie linie prawdziwego transkryptu zaznaczonej sesji.
    #[test]
    fn peek_panel_shows_lines_from_the_real_transcript() {
        let _guard = test_guard();
        let ws = list_ws("peek-lines");
        let app = list_app_for(&ws);
        let text = render(&app, 120, 38);
        assert!(text.contains("peek"), "podpis panelu: {text}");
        assert!(text.contains("write alfa.txt"), "tytul sesji w panelu: {text}");
        assert!(text.contains("Writing alfa.txt"), "wywolanie w panelu: {text}");
        assert!(text.contains("Wrote alfa.txt"), "wynik w panelu: {text}");
        assert!(text.contains("DONE"), "podsumowanie w panelu: {text}");

        // Waski terminal: panel znika, lista zostaje czytelna.
        let narrow = render(&app, 40, 20);
        assert!(!narrow.contains("peek"), "przy 40 kolumnach nie ma panelu");
        assert!(narrow.contains("write alfa.txt"), "lista zostaje: {narrow}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Bez sesji panel zostaje, ale pokazuje podpis zamiast zmyślonych linii.
    #[test]
    fn peek_panel_without_a_session_says_so() {
        let _guard = test_guard();
        let ws = temp_ws("peek-empty");
        let app = list_app_for(&ws);
        let text = render(&app, 120, 38);
        assert!(text.contains("brak sesji"), "pusty panel ma podpis: {text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn wrap_never_exceeds_width() {
        let _guard = test_guard();
        for width in [8usize, 20, 40, 80] {
            for line in wrap("ala ma kota a kot ma ale i to bardzo dlugie zdanie", width) {
                assert!(line.chars().count() <= width, "{} > {}", line, width);
            }
        }
        assert_eq!(wrap("", 10), vec![String::new()]);
    }

    // ── Runda 14: licznik z metadanych, `/model`, rejestr komend ────────────

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn type_text(app: &mut App, text: &str) {
        for ch in text.chars() {
            press(app, KeyCode::Char(ch));
        }
    }

    fn top_line(app: &App) -> String {
        render(app, 120, 34)
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_string()
    }

    /// Brak metadanych to `?`, nie zmyślony limit. Licznik jest wtedy
    /// szacunkiem z znaków z jawnym `~`.
    #[test]
    fn counter_without_model_metadata_shows_a_question_mark() {
        let _guard = test_guard();
        let ws = temp_ws("counter-missing");
        let mut app = App::new(Options::new(ws.clone()));
        app.view = View::Session;
        app.context_chars = 6000;
        let line = top_line(&app);
        assert!(
            line.trim_end().ends_with(" / ?"),
            "brak limitu to `?`: {line:?}"
        );
        assert!(line.contains("~1.5K"), "szacunek z tylda: {line:?}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// `limit.context` z `/models` trafia do licznika: `1.0M` zamiast `500K`,
    /// a realne `usage` zdejmuje tyldę.
    #[test]
    fn counter_uses_the_context_limit_and_real_usage() {
        let _guard = test_guard();
        let ws = temp_ws("counter-limit");
        let mut app = App::new(Options::new(ws.clone()));
        app.view = View::Session;
        app.catalog = models::demo_catalog();
        app.model = "mimo-v2.6-flash".to_string();
        app.context_chars = 6000;
        let line = top_line(&app);
        assert!(line.contains("~1.5K / 1.0M"), "limit z metadanych: {line:?}");

        // Provider przysłał `usage`: licznik bez tyldy.
        app.context_tokens = Some(1536);
        let line = top_line(&app);
        assert!(line.contains("1.5K / 1.0M"), "realne tokeny: {line:?}");
        assert!(!line.contains("~1.5K"), "bez tyldy przy usage: {line:?}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// `/model`: modal pokazuje wszystkie modele z listy (nazwa, id, kontekst,
    /// cena, znacznik aktywnego), `Enter` przełącza, plik config zostaje.
    #[test]
    fn model_picker_lists_every_model_and_switches_the_session() {
        let _guard = test_guard();
        let ws = temp_ws("picker");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Session;
        let mut app = App::new(opts);
        app.catalog = models::demo_catalog();
        app.model = "mimo-v2.6-flash".to_string();

        type_text(&mut app, "/model");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.picker, Some(0), "picker otwarty na aktywnym modelu");

        let text = render(&app, 120, 38);
        assert!(text.contains("Select model"), "pasek tytulu: {text}");
        assert!(text.contains("Esc:back"), "stopka klawiszy: {text}");
        for model in &app.catalog.models {
            assert!(text.contains(&model.id), "brak {} w liscie", model.id);
        }
        assert!(text.contains("1.0M"), "okno kontekstu: {text}");
        assert_eq!(text.matches('\u{25CF}').count(), 1, "jeden znacznik aktywnego");

        press(&mut app, KeyCode::Down);
        assert_eq!(app.picker, Some(1));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.picker, None, "Enter zamyka picker");
        assert_eq!(app.model, "mimo-v2.6-pro", "model przelaczony w sesji");
        assert_eq!(
            models::config_get(&ws, "model").as_deref(),
            Some("mimo-v2.6-pro"),
            "wybor zapisany w .agent/config"
        );

        // Restart procesu: nowy odczyt pliku daje ten sam model,
        // a `ARNDTAI_MODEL` (z-env) nadal wygrywa z plikiem.
        assert_eq!(models::resolve_model(&ws, None, "domyslny"), "mimo-v2.6-pro");
        assert_eq!(models::resolve_model(&ws, Some("z-env"), "domyslny"), "z-env");

        // Podpis composera pokazuje nowy model.
        let text = render(&app, 120, 38);
        assert!(text.contains("mimo-v2.6-pro"), "model w stopce composera: {text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Zmiana modelu dopisuje `note` do transkryptu bieżącej sesji.
    #[test]
    fn switching_model_writes_a_note_event_to_the_transcript() {
        let _guard = test_guard();
        let ws = temp_ws("picker-note");
        let session = context::Session::start(&ws, "zadanie");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Session;
        let mut app = App::new(opts);
        app.catalog = models::demo_catalog();
        app.model = "mimo-v2.6-flash".to_string();
        app.session = Some(session.id.clone());

        app.open_picker();
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);

        let transcript = std::fs::read_to_string(&session.file).unwrap_or_default();
        assert!(
            transcript.contains("\"kind\":\"note\"") && transcript.contains("\"model\":\"mimo-v2.6-pro\""),
            "note w transkrypcie: {transcript}"
        );
        assert!(
            app.entries.iter().any(|e| e.subject.contains("model: mimo-v2.6-pro")),
            "wpis w scrollbacku"
        );
        // Nowy `kind` nie psuje listy sesji: tytuł dalej pochodzi z `task`.
        let rows = sessions::scan(&ws);
        assert_eq!(rows.len(), 1, "sesja z `note` nadal na liscie");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// `/help` wypisuje sześć komend w jednym bloku, a nieznana komenda daje
    /// komunikat zamiast paniki.
    #[test]
    fn help_lists_all_commands_and_unknown_commands_do_not_crash() {
        let _guard = test_guard();
        let ws = temp_ws("help");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Session;
        let mut app = App::new(opts);

        type_text(&mut app, "/help");
        press(&mut app, KeyCode::Enter);
        let help = app
            .entries
            .last()
            .map(|e| e.subject.clone())
            .unwrap_or_default();
        for command in commands::all() {
            assert!(help.contains(command.name), "brak {} w /help", command.name);
        }
        assert_eq!(help.lines().count(), 12, "naglowek + 11 komend: {help}");

        let text = render(&app, 120, 38);
        assert!(text.contains("Pokaz dostepne komendy"), "tabela w scrollbacku: {text}");

        type_text(&mut app, "/nie-ma");
        press(&mut app, KeyCode::Enter);
        let last = app.entries.last().map(|e| e.subject.clone()).unwrap_or_default();
        assert!(last.contains("nieznana komenda /nie-ma"), "{last}");
        assert!(!app.quit, "nieznana komenda nie konczy programu");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Prefiks `/` pokazuje podpowiedzi; `Tab` uzupełnia, `Esc` chowa.
    #[test]
    fn slash_shows_hints_and_tab_completes_them() {
        let _guard = test_guard();
        let ws = temp_ws("hints");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Session;
        let mut app = App::new(opts);

        press(&mut app, KeyCode::Char('/'));
        assert_eq!(app.hints().len(), 11, "sam `/` pokazuje wszystkie komendy");
        let text = render(&app, 120, 38);
        assert!(text.contains("/settings"), "lista podpowiedzi: {text}");

        type_text(&mut app, "mo");
        assert_eq!(app.hints().len(), 1);
        assert!(app.hints()[0].name == "/model");
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.input, "/model", "Tab uzupelnia do /model");

        // `Esc` chowa podpowiedzi, nie wychodząc z sesji.
        app.input = "/".to_string();
        app.cursor = 1;
        app.hints_hidden = false;
        press(&mut app, KeyCode::Esc);
        assert!(app.hints_hidden, "Esc chowa podpowiedzi");
        assert_eq!(app.view, View::Session, "Esc nie wychodzi z sesji");
        let text = render(&app, 120, 38);
        assert!(!text.contains("Ustawienia (w kolejnej rundzie)"), "brak listy: {text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// `/model` z ekranu powitalnego otwiera sesję i picker — nie zostawia
    /// niewidocznego modala pod ekranem powitalnym.
    #[test]
    fn model_from_the_welcome_screen_opens_a_session_and_the_picker() {
        let _guard = test_guard();
        let ws = temp_ws("picker-welcome");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Welcome;
        let mut app = App::new(opts);
        app.catalog = models::demo_catalog();

        type_text(&mut app, "/model");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::Session, "komenda wchodzi w sesje");
        assert_eq!(app.picker, Some(0), "picker widoczny");
        let text = render(&app, 120, 38);
        assert!(text.contains("Select model"), "{text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Bez listy modeli `/model` mówi to wprost (zamiast pustego modala).
    #[test]
    fn model_without_metadata_says_so_and_leaves_the_model_alone() {
        let _guard = test_guard();
        let ws = temp_ws("picker-empty");
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Session;
        let mut app = App::new(opts);
        assert!(app.catalog.is_empty(), "offline nie ma listy modeli");
        let before = app.model.clone();

        type_text(&mut app, "/model");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.picker, None, "brak pustego modala");
        assert_eq!(app.model, before, "model bez zmian");
        let last = app.entries.last().map(|e| e.subject.clone()).unwrap_or_default();
        assert!(last.contains("brak listy modeli"), "{last}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    // ── Runda 15: `/settings`, tryb `ask`, motyw `jasny` ───────────────────

    #[test]
    fn settings_modal_shows_six_keys_and_writes_the_file() {
        let _guard = test_guard();
        let ws = temp_ws("settings-modal");
        let mut app = App::new(Options::new(ws.clone()));
        app.view = View::Session;
        app.input = "/settings".to_string();
        app.submit();
        assert_eq!(app.settings_sel, Some(0), "/settings otwiera modal");

        let text = render(&app, 100, 38);
        assert!(text.contains("Settings"), "pasek tytulu: {text}");
        for label in [
            "motyw",
            "tryb zgod",
            "poziom pamieci",
            "znaczniki czasu",
            "raportowanie myszy",
            "limit odpowiedzi",
        ] {
            assert!(text.contains(label), "brak {label} w modalu");
        }
        for value in ["groknight", "always-approve", "lite", "on", "4096"] {
            assert!(text.contains(value), "brak wartosci {value} w modalu");
        }
        assert!(text.contains("Esc:close"), "stopka klawiszy: {text}");

        // `→` na motywie: jasny, od razu i w pliku.
        app.on_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.settings.theme, Theme::Light);
        assert_eq!(
            models::config_get(&ws, "theme").as_deref(),
            Some("jasny"),
            "wybor trafia do .agent/config"
        );
        assert!(render(&app, 100, 38).contains("jasny"));

        // `↓` + `Enter`: tryb zgod `ask`, podpis composera idzie za nim.
        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.settings.approval, Approval::Ask);
        assert_eq!(app.mode, "ask");
        assert_eq!(models::config_get(&ws, "approval").as_deref(), Some("ask"));

        // `←` wraca do poprzedniej wartosci; `Esc` zamyka modal.
        app.on_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(app.settings.approval, Approval::AlwaysApprove);
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.settings_sel, None, "Esc zamyka modal");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn settings_from_a_broken_file_warn_in_the_scrollback() {
        let _guard = test_guard();
        let ws = temp_ws("settings-warn");
        std::fs::create_dir_all(ws.join(".agent")).unwrap();
        std::fs::write(ws.join(".agent").join("config"), "theme = neon\nmouse = moze\n").unwrap();
        let app = App::new(Options::new(ws.clone()));
        assert_eq!(app.settings.theme, Theme::Groknight, "domyslna wartosc");
        let text: String = app
            .entries
            .iter()
            .map(|entry| entry.subject.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("theme = 'neon'"),
            "ostrzezenie w scrollbacku: {text}"
        );
        assert!(text.contains("mouse = 'moze'"), "{text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn theme_command_switches_the_palette_immediately() {
        let _guard = test_guard();
        let ws = temp_ws("theme-cmd");
        let mut app = App::new(Options::new(ws.clone()));
        app.view = View::Session;
        app.input = "/theme jasny".to_string();
        app.submit();
        assert_eq!(app.settings.theme, Theme::Light);
        assert_eq!(models::config_get(&ws, "theme").as_deref(), Some("jasny"));
        // Literowka nie crashuje i nie zmienia motywu.
        app.input = "/theme neon".to_string();
        app.submit();
        assert_eq!(app.settings.theme, Theme::Light);
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn light_theme_repaints_background_and_accents_and_groknight_comes_back() {
        let _guard = test_guard();
        let ws = temp_ws("theme-color");
        let mut app = App::new(Options::new(ws.clone()));
        app.entries.push(Entry::new(Kind::Task, "", "zadanie".to_string()));
        app.entries.push(Entry::new(Kind::Thought, "", "mysl".to_string()));

        let dark = render_buffer(&app, 100, 30);
        assert_eq!(dark.cell((0, 0)).unwrap().bg, BG, "groknight: tlo #141414");
        assert!(
            dark.content.iter().any(|cell| cell.fg == MAGENTA),
            "groknight: akcent magenta"
        );

        app.settings.theme = Theme::Light;
        let light = render_buffer(&app, 100, 30);
        assert_eq!(
            light.cell((0, 0)).unwrap().bg,
            crate::theme::LIGHT_BG,
            "jasny: tlo #f5f5f5"
        );
        assert!(
            light
                .content
                .iter()
                .any(|cell| cell.fg == crate::theme::LIGHT_ACCENT),
            "jasny: akcent #7c3aed"
        );
        let bg_cells = light
            .content
            .iter()
            .filter(|cell| cell.bg == crate::theme::LIGHT_BG)
            .count();
        assert!(bg_cells > 100, "tlo pokrywa ekran: {bg_cells}");

        app.settings.theme = Theme::Groknight;
        let back = render_buffer(&app, 100, 30);
        assert_eq!(back.cell((0, 0)).unwrap().bg, BG, "groknight wraca");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn timestamps_follow_the_setting() {
        let _guard = test_guard();
        let ws = temp_ws("timestamps");
        let mut app = App::new(Options::new(ws.clone()));
        app.entries.push(Entry::new(Kind::Task, "", "zadanie".to_string()));
        let with_stamp = app.rows(80);
        assert!(
            with_stamp.iter().any(|row| row.right.is_some()),
            "godzina przy wpisie uzytkownika"
        );
        app.settings.timestamps = false;
        let without = app.rows(80);
        assert!(
            without.iter().all(|row| row.right.is_none()),
            "znaczniki off: bez godziny"
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn ask_question_waits_in_the_composer_for_t_or_n() {
        let _guard = test_guard();
        let ws = temp_ws("ask-keys");
        let mut app = App::new(Options::new(ws.clone()));
        let (approver, handle) = agent::Approver::channel();
        app.approval = Some(handle);
        // Agent pyta z innego watku — dokladnie tak, jak robi to `agent::run`.
        let (answer_tx, answer_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let allowed = approver.ask(agent::ApprovalRequest {
                step: 1,
                tool: "fs.write".to_string(),
                arg: "nota.txt".to_string(),
            });
            let _ = answer_tx.send(allowed);
        });

        // Pytanie pojawia sie w composerze dopiero po `poll`.
        let mut question = None;
        for _ in 0..200 {
            app.poll();
            if app.question.is_some() {
                question = app.question.clone();
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let question = question.expect("prosba o zgode musi dotrzec do UI");
        assert_eq!(question.tool, "fs.write");

        let text = render(&app, 100, 30);
        assert!(
            text.contains("Zezwoli\u{0107} na fs.write nota.txt? [t/n]"),
            "pytanie w composerze: {text}"
        );

        // `n` = odmowa: agent dostaje `false`, pytanie znika.
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert_eq!(
            answer_rx.recv_timeout(Duration::from_secs(2)),
            Ok(false),
            "odmowa dochodzi do watku petli"
        );
        assert!(app.question.is_none());
        let text = render(&app, 100, 30);
        assert!(!text.contains("[t/n]"), "po odpowiedzi wraca composer: {text}");
        let _ = std::fs::remove_dir_all(&ws);
    }


    // ── Runda 16: skille ────────────────────────────────────────────────────

    fn seed_skill(ws: &Path, dir: &str, header: &str, body: &str) {
        let target = ws.join("skills").join(dir);
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("SKILL.md"), format!("{}\n{}\n", header, body)).unwrap();
    }

    /// Sesja na workspace z dwoma skillami: `raport` i `notatki`.
    fn skills_app(tag: &str) -> (App, PathBuf) {
        let ws = temp_ws(tag);
        seed_skill(
            &ws,
            "raport",
            "---\nname: raport\ndescription: dzienny raport z postepu\n---",
            "# Raport\n\nTrzy liczby na dzien.",
        );
        seed_skill(
            &ws,
            "notatki",
            "---\nname: notatki\ndescription: zapisuje notatki i decyzje\n---",
            "Decyzja, powod, data.",
        );
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Session;
        let app = App::new(opts);
        (app, ws)
    }

    #[test]
    fn skills_modal_lists_name_description_and_size() {
        let _guard = test_guard();
        let (mut app, ws) = skills_app("skills-modal");
        type_text(&mut app, "/skills");
        press(&mut app, KeyCode::Enter);
        assert!(app.skills_sel.is_some(), "modal /skills otwarty");
        let text = render(&app, 120, 38);
        assert!(text.contains("Skills"), "tytul modala: {text}");
        for name in ["raport", "notatki"] {
            assert!(text.contains(name), "brak {name} w modalu: {text}");
        }
        assert!(
            text.contains("dzienny raport z postepu"),
            "opis skilla widoczny: {text}"
        );
        assert!(text.contains("B"), "rozmiar tresci widoczny: {text}");
        press(&mut app, KeyCode::Esc);
        assert!(app.skills_sel.is_none(), "Esc zamyka modal");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn skill_command_loads_content_into_the_next_turn() {
        let _guard = test_guard();
        let (mut app, ws) = skills_app("skill-load");
        // Status bar liczy odkryte skille.
        let status = render(&app, 120, 38);
        assert!(status.contains("skille 2"), "licznik w pasku: {status}");

        type_text(&mut app, "/skill raport");
        press(&mut app, KeyCode::Enter);
        let last = app
            .entries
            .last()
            .map(|e| e.subject.clone())
            .unwrap_or_default();
        assert!(
            last.contains("raport") && last.contains("w kontekscie"),
            "potwierdzenie w scrollbacku: {last}"
        );
        // Treść idzie do kontekstu dopiero teraz — przez `Config`.
        let cfg = app.config();
        assert_eq!(cfg.skill_context.len(), 1, "{:?}", cfg.skill_context);
        assert_eq!(cfg.skill_context[0].0, "raport");
        assert!(
            cfg.skill_context[0].1.contains("Trzy liczby na dzien."),
            "tresc skilla w kontekscie: {:?}",
            cfg.skill_context[0].1
        );
        // Modal pokazuje, że treść jest już w kontekście.
        type_text(&mut app, "/skills");
        press(&mut app, KeyCode::Enter);
        let text = render(&app, 120, 38);
        assert!(text.contains("w kontek"), "znacznik w modalu: {text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn unknown_skill_does_not_crash_and_says_so() {
        let _guard = test_guard();
        let (mut app, ws) = skills_app("skill-404");
        type_text(&mut app, "/skill nie-ma");
        press(&mut app, KeyCode::Enter);
        assert!(!app.running, "nieznana nazwa nie startuje petli");
        let last = app
            .entries
            .last()
            .map(|e| e.subject.clone())
            .unwrap_or_default();
        assert!(last.contains("nie znam skilla"), "komunikat: {last}");
        assert!(app.config().skill_context.is_empty());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn enter_in_the_modal_previews_the_body_in_the_scrollback() {
        let _guard = test_guard();
        let (mut app, ws) = skills_app("skill-preview");
        type_text(&mut app, "/skills");
        press(&mut app, KeyCode::Enter);
        // Lista sortowana po nazwie: pierwszy wiersz to `notatki`.
        press(&mut app, KeyCode::Enter); // podglad zaznaczonego
        assert!(app.skills_sel.is_none(), "podglad zamyka modal");
        let last = app
            .entries
            .last()
            .map(|e| e.subject.clone())
            .unwrap_or_default();
        assert!(
            last.contains("skill 'notatki'") && last.contains("Decyzja, powod, data."),
            "tresc w scrollbacku: {last}"
        );
        // Podgląd nie wczytuje treści do kontekstu.
        assert!(app.config().skill_context.is_empty());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn broken_header_skill_is_listed_but_not_loaded() {
        let _guard = test_guard();
        let ws = temp_ws("skill-broken-tui");
        seed_skill(
            &ws,
            "zepsuty",
            "---\nname: zepsuty\nto nie jest yaml\n---",
            "tresc",
        );
        let mut opts = Options::new(ws.clone());
        opts.start = Start::Session;
        let mut app = App::new(opts);
        type_text(&mut app, "/skill zepsuty");
        press(&mut app, KeyCode::Enter);
        let last = app
            .entries
            .last()
            .map(|e| e.subject.clone())
            .unwrap_or_default();
        assert!(last.contains("blad naglowka"), "komunikat: {last}");
        assert!(app.config().skill_context.is_empty(), "nie wstrzykniety");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn skill_loaded_by_the_model_is_marked_in_the_modal() {
        let _guard = test_guard();
        let (mut app, ws) = skills_app("skill-tool");
        app.handle_event(serde_json::json!({
            "kind": "tool_call",
            "data": { "step": 1, "tool": "skill.load", "args": { "name": "notatki" } }
        }));
        assert!(
            app.loaded_skills.iter().any(|n| n == "notatki"),
            "skill.load modelu liczy sie jako w kontekscie"
        );
        let text = render(&app, 120, 38);
        assert!(text.contains("Loading"), "naglowek wywolania: {text}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    /// Trzy wiersze z „alfa“, żeby licznik i `n` miały po czym chodzić.
    fn search_app(tag: &str) -> (App, PathBuf) {
        let ws = temp_ws(tag);
        let mut app = App::new(Options::new(ws.clone()));
        app.search_width.set(99);
        app.search_view.set(10);
        app.entries.push(Entry::new(Kind::Task, "", "zadanie bez frazy"));
        app.entries.push(Entry::new(Kind::Info, "", "pierwsze alfa"));
        app.entries.push(Entry::new(Kind::Info, "", "wiersz bez frazy"));
        app.entries.push(Entry::new(Kind::Info, "", "drugie alfa"));
        app.entries.push(Entry::new(Kind::Info, "", "trzecie alfa"));
        (app, ws)
    }

    #[test]
    fn ctrl_f_counts_hits_and_n_walks_them() {
        let _guard = test_guard();
        let (mut app, ws) = search_app("search-hits");
        app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert!(app.search.is_some(), "Ctrl+F otwiera modal");

        // Puste zapytanie: sam pasek, bez listy i bez bledu.
        let empty = render(&app, 100, 30);
        assert!(empty.contains("Search scrollback"), "{empty}");
        assert!(empty.contains("/ szukaj"), "pusty pasek: {empty}");
        assert!(!empty.contains("brak trafien"), "pusto to nie brak trafien");

        type_text(&mut app, "alfa");
        let hits = app.search_hits();
        assert_eq!(hits.len(), 3, "trzy wiersze z fraza: {hits:?}");
        let text = render(&app, 100, 30);
        assert!(text.contains("1/3"), "licznik od pierwszego trafienia: {text}");
        assert!(text.contains("pierwsze alfa"), "podglad trafienia: {text}");

        // `n` w trybie pisania to litera; `Enter` zamyka modal, zostawia
        // podswietlone trafienie i oddaje `n`/`N` stopce.
        press(&mut app, KeyCode::Enter);
        assert!(!app.search.as_ref().unwrap().visible, "Enter zamyka modal");
        let text = render(&app, 100, 30);
        assert!(text.contains("szukanie: alfa"), "pasek szukania w stopce: {text}");
        assert!(text.contains("1/3"), "licznik w stopce: {text}");
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.search.as_ref().unwrap().current, 1);
        let text = render(&app, 100, 30);
        assert!(text.contains("2/3"), "po `n` drugie trafienie: {text}");
        press(&mut app, KeyCode::Char('N'));
        assert_eq!(app.search.as_ref().unwrap().current, 0);

        // `Esc` zamyka i wraca na miejsce, z ktorego przyszlo szukanie.
        press(&mut app, KeyCode::Esc);
        assert!(app.search.is_none());
        assert_eq!(app.scroll, 0, "Esc wraca do miejsca otwarcia");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn search_without_hits_says_brak_trafien() {
        let _guard = test_guard();
        let (mut app, ws) = search_app("search-none");
        app.on_key(KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL));
        type_text(&mut app, "zzz");
        let text = render(&app, 100, 30);
        assert!(text.contains("brak trafien"), "{text}");
        assert!(text.contains("0/0"), "licznik bez trafien: {text}");
        // Esc po pustym wyniku nie crashuje i wraca do scrollbacku.
        press(&mut app, KeyCode::Esc);
        assert!(app.search.is_none());
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn highlight_reverses_only_the_match() {
        let spans = vec![Span::styled(
            "ala ma ala",
            Style::default().fg(FG).bg(BG),
        )];
        let out = highlight_spans(&spans, "ala");
        let text: String = out.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "ala ma ala", "podswietlenie nie zmienia tekstu");
        let reversed: Vec<&str> = out
            .iter()
            .filter(|span| span.style.add_modifier.contains(Modifier::REVERSED))
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(reversed, vec!["ala", "ala"], "tylko trafienia odwrocone");
        // Bez zmiany palety: kolory zostaja te same.
        assert!(out.iter().all(|span| span.style.fg == Some(FG) && span.style.bg == Some(BG)));
    }

    #[test]
    fn copy_takes_the_last_assistant_block_through_osc52() {
        let _guard = test_guard();
        let (mut app, ws) = search_app("copy-answer");
        app.entries.push(
            Entry::new(Kind::Summary, "", "DONE")
                .body(Some("gotowe: trzy kroki".to_string())),
        );
        app.copy_block(false);
        let message = app.entries.last().unwrap().subject.clone();
        assert_eq!(
            message,
            format!("skopiowano {} znakow (OSC 52)", "gotowe: trzy kroki".chars().count()),
            "potwierdzenie z liczba znakow: {message}"
        );
        assert!(!message.contains('◆') && !message.contains('❯'), "bez znacznikow blokow");

        // `/copy all`: cala rozmowa w formacie `ty: tresc`.
        app.copy_block(true);
        assert!(app.transcript_text().contains("ty: zadanie bez frazy"));
        assert!(app.transcript_text().contains("asystent: DONE gotowe: trzy kroki"));

        // Bez odpowiedzi asystenta: komunikat, nie crash.
        let mut empty = App::new(Options::new(temp_ws("copy-empty")));
        empty.copy_block(false);
        assert_eq!(
            empty.entries.last().unwrap().subject,
            "brak odpowiedzi asystenta do skopiowania"
        );
        let _ = std::fs::remove_dir_all(&ws);
        let _ = std::fs::remove_dir_all(&empty.workspace);
    }

    fn export_app(tag: &str) -> (App, PathBuf) {
        let ws = temp_ws(tag);
        let mut app = App::new(Options::new(ws.clone()));
        app.model = "mimo-v2.6-flash".to_string();
        app.entries.push(Entry::new(Kind::Task, "", "write a.txt :: alfa"));
        app.entries
            .push(Entry::new(Kind::Thought, "", "najpierw zapis").step(1));
        let mut call = Entry::new(Kind::Call, "Writing", "a.txt").step(1);
        call.tool = "fs.write".to_string();
        app.entries.push(call);
        let mut ok = Entry::new(Kind::Ok, "Wrote", "a.txt")
            .detail("+4 B")
            .step(1);
        ok.tool = "fs.write".to_string();
        ok.body = Some("alfa".to_string());
        app.entries.push(ok);
        app.status = Some("DONE".to_string());
        app.steps = 1;
        app.tool_calls = 1;
        (app, ws)
    }

    #[test]
    fn export_writes_header_steps_and_final_status() {
        let _guard = test_guard();
        let (mut app, ws) = export_app("export-ok");
        app.export(Some("raport.md"));
        let text = std::fs::read_to_string(ws.join("raport.md")).expect("plik eksportu");
        assert!(text.starts_with("# write a.txt :: alfa"), "{text}");
        assert!(text.contains("Data: "), "data w naglowku: {text}");
        assert!(text.contains("Model: mimo-v2.6-flash"), "{text}");
        assert!(text.contains("## krok 1"), "blok kroku: {text}");
        assert!(text.contains("mysl: najpierw zapis"), "{text}");
        assert!(text.contains("wywolanie: fs.write a.txt"), "{text}");
        assert!(text.contains("status: ok"), "{text}");
        assert!(text.contains("## status koncowy"), "{text}");
        assert!(text.contains("DONE"), "status koncowy: {text}");

        // Bez argumentu: `eksport-YYYYMMDD-HHMM.md` w workspace.
        app.export(None);
        let named = std::fs::read_dir(&ws)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .find(|name| name.starts_with("eksport-") && name.ends_with(".md"))
            .expect("domyslna nazwa eksportu");
        assert_eq!(named.len(), "eksport-YYYYMMDD-HHMM.md".len(), "{named}");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn export_refuses_a_path_with_parent_dir() {
        let _guard = test_guard();
        let (mut app, ws) = export_app("export-deny");
        let outside = ws.parent().unwrap().join("ucieczka-arndtai.md");
        let _ = std::fs::remove_file(&outside);
        app.export(Some("../ucieczka-arndtai.md"));
        assert!(!outside.exists(), "eksport nie moze wyjsc z workspace");
        assert!(
            app.entries
                .last()
                .map(|entry| entry.subject.contains("eksport odrzucony"))
                .unwrap_or(false),
            "komunikat odmowy: {:?}",
            app.entries.last().map(|entry| &entry.subject)
        );
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn export_result_is_cut_to_400_chars() {
        let _guard = test_guard();
        let (mut app, ws) = export_app("export-cut");
        app.entries[3].body = Some("x".repeat(900));
        let text = app.export_markdown();
        let line = text
            .lines()
            .find(|line| line.starts_with("wynik: "))
            .expect("linia wyniku");
        assert!(
            line.chars().count() <= 407,
            "wynik skrocony do 400 znakow (+ `wynik: `): {} znakow",
            line.chars().count()
        );
        let _ = std::fs::remove_dir_all(&ws);
    }
}
