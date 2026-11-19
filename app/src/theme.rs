//! Motyw `jasny` (runda 15): drugi zestaw kolorów bez ruszania układu.
//!
//! Układ, metryki i wszystkie reguły rysowania zostają w `tui.rs` — motyw to
//! wyłącznie paleta. Zamiast drugiego zestawu `Style` w każdym miejscu, obraz
//! rysowany jest paletą `groknight`, a na koniec jednym przebiegiem podmieniamy
//! kolory w buforze (`recolor`). Dzięki temu nie da się „zapomnieć" o żadnym
//! elemencie: nowy element dostaje kolor z palety i automatycznie ma wariant
//! jasny.
//!
//! Kontrasty w motywie jasnym są dobrane pod 16 kolorów terminala: bez
//! przezroczystości, gradientów i odcieni, które przy kwantyzacji zlewają się
//! z tłem. Tło `#f5f5f5`, tekst `#1f1f1f`, akcent magenta `#7c3aed`, zieleń
//! `#2f7d32`.

use crate::tui::{
    BG, BG_LIGHT, BG_TERMINAL, CYAN, FG, FG_DARK, GRAY, GRAY_BRIGHT, GRAY_DIM, GREEN, GREEN_DARK,
    MAGENTA, ORANGE, PROMPT_BORDER, PROMPT_BORDER_ACTIVE, PURPLE, RED, SELECTION_BORDER, TEAL,
    YELLOW,
};
use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// Tło całego ekranu w motywie jasnym (spec: `#f5f5f5`).
pub const LIGHT_BG: Color = Color::Rgb(245, 245, 245);
/// Tekst główny (spec: `#1f1f1f`).
pub const LIGHT_FG: Color = Color::Rgb(31, 31, 31);
/// Akcent magenta (spec: `#7c3aed`).
pub const LIGHT_ACCENT: Color = Color::Rgb(124, 58, 237);
/// Zieleń potwierdzeń (spec: `#2f7d32`).
pub const LIGHT_GREEN: Color = Color::Rgb(47, 125, 50);

/// Pas zaznaczenia — jaśniejszy od tła, ale nadal odcinający się od treści.
const LIGHT_BG_LIGHT: Color = Color::Rgb(232, 232, 236);
/// Tło paska przewijania (w `groknight` ciemniejsze od tła).
const LIGHT_BG_TERMINAL: Color = Color::Rgb(250, 250, 250);
const LIGHT_FG_DARK: Color = Color::Rgb(45, 45, 45);
const LIGHT_GRAY: Color = Color::Rgb(100, 100, 100);
const LIGHT_GRAY_DIM: Color = Color::Rgb(120, 120, 120);
const LIGHT_GRAY_BRIGHT: Color = Color::Rgb(80, 80, 80);
const LIGHT_ORANGE: Color = Color::Rgb(176, 74, 0);
const LIGHT_RED: Color = Color::Rgb(183, 28, 58);
const LIGHT_TEAL: Color = Color::Rgb(0, 110, 95);
const LIGHT_YELLOW: Color = Color::Rgb(138, 90, 0);
const LIGHT_CYAN: Color = Color::Rgb(0, 92, 153);
const LIGHT_PURPLE: Color = Color::Rgb(90, 50, 160);
const LIGHT_SELECTION_BORDER: Color = Color::Rgb(203, 203, 210);
const LIGHT_PROMPT_BORDER: Color = Color::Rgb(205, 205, 212);
const LIGHT_PROMPT_BORDER_ACTIVE: Color = Color::Rgb(150, 150, 160);
/// Pas wstawionej linii diffu: w `groknight` ciemna zieleń, tu jasna.
const LIGHT_GREEN_DARK: Color = Color::Rgb(214, 238, 214);

/// Paleta `groknight` → paleta `jasny`. Każdy kolor używany przez `tui.rs`
/// i `welcome.rs` ma tu swój odpowiednik.
pub fn light(color: Color) -> Color {
    match color {
        BG => LIGHT_BG,
        BG_LIGHT => LIGHT_BG_LIGHT,
        BG_TERMINAL => LIGHT_BG_TERMINAL,
        FG => LIGHT_FG,
        // `FG_DARK` i `ACCENT_USER` mają tę samą wartość (200,200,200) — jeden
        // wariant pokrywa oba.
        FG_DARK => LIGHT_FG_DARK,
        GRAY => LIGHT_GRAY,
        GRAY_DIM => LIGHT_GRAY_DIM,
        GRAY_BRIGHT => LIGHT_GRAY_BRIGHT,
        GREEN => LIGHT_GREEN,
        MAGENTA => LIGHT_ACCENT,
        ORANGE => LIGHT_ORANGE,
        RED => LIGHT_RED,
        TEAL => LIGHT_TEAL,
        YELLOW => LIGHT_YELLOW,
        CYAN => LIGHT_CYAN,
        PURPLE => LIGHT_PURPLE,
        SELECTION_BORDER => LIGHT_SELECTION_BORDER,
        PROMPT_BORDER => LIGHT_PROMPT_BORDER,
        PROMPT_BORDER_ACTIVE => LIGHT_PROMPT_BORDER_ACTIVE,
        GREEN_DARK => LIGHT_GREEN_DARK,
        // Terminal nie ustawia ani tła, ani koloru znaku w każdym miejscu
        // (spacje, wiersze bez własnego stylu) — w motywie jasnym muszą iść
        // za paletą, inaczej zostałyby białe na jasnym tle.
        Color::Reset => LIGHT_FG,
        // Kolory spoza palety (np. z wypełnienia bufora) obracamy jasnością:
        // ciemne rozjaśniamy, jasne przyciemniamy. Kontrast zostaje.
        other => invert(other),
    }
}

/// Awaryjna podmiana koloru spoza palety: zachowuje odcień, odwraca jasność.
fn invert(color: Color) -> Color {
    let (r, g, b) = match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black | Color::DarkGray => (0, 0, 0),
        Color::Red => (200, 0, 0),
        Color::Green => (0, 160, 0),
        Color::Yellow => (200, 160, 0),
        Color::Blue => (0, 0, 200),
        Color::Magenta => (170, 0, 170),
        Color::Cyan => (0, 150, 150),
        Color::Gray | Color::White => (220, 220, 220),
        _ => (128, 128, 128),
    };
    let mix = |c: u8, target: u8| -> u8 {
        ((c as u16 * 35 + target as u16 * 65) / 100) as u8
    };
    let luminance = (r as u32 * 30 + g as u32 * 59 + b as u32 * 11) / 100;
    if luminance > 128 {
        // Jasne tło/akcent: przyciemnij w stronę czerni, żeby był czytelny.
        Color::Rgb(mix(r, 20), mix(g, 20), mix(b, 20))
    } else {
        Color::Rgb(mix(r, 240), mix(g, 240), mix(b, 240))
    }
}

/// Podmienia kolory całego bufora na paletę jasną. Wywoływane raz na klatkę,
/// po narysowaniu wszystkich elementów.
pub fn recolor(buf: &mut Buffer) {
    for cell in buf.content.iter_mut() {
        cell.fg = light(cell.fg);
        cell.bg = light(cell.bg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_color_gets_a_different_light_value() {
        for color in [
            BG,
            BG_LIGHT,
            BG_TERMINAL,
            FG,
            FG_DARK,
            GRAY,
            GRAY_DIM,
            GRAY_BRIGHT,
            GREEN,
            MAGENTA,
            ORANGE,
            RED,
            TEAL,
            YELLOW,
            CYAN,
            PURPLE,
            SELECTION_BORDER,
            PROMPT_BORDER,
            PROMPT_BORDER_ACTIVE,
            GREEN_DARK,
        ] {
            assert_ne!(light(color), color, "{:?} nie moze zostac bez zmian", color);
        }
        assert_eq!(light(BG), LIGHT_BG);
        assert_eq!(light(FG), LIGHT_FG);
        assert_eq!(light(MAGENTA), LIGHT_ACCENT);
        assert_eq!(light(GREEN), LIGHT_GREEN);
    }

    #[test]
    fn unknown_colors_keep_contrast_against_the_light_background() {
        // Ciemny kolor spoza palety staje się jasny, jasny — ciemny.
        let dark = light(Color::Rgb(10, 20, 30));
        let bright = light(Color::Rgb(250, 250, 250));
        let lum = |c: Color| match c {
            Color::Rgb(r, g, b) => (r as u32 * 30 + g as u32 * 59 + b as u32 * 11) / 100,
            _ => 0,
        };
        assert!(lum(dark) > 128, "ciemny ma pojsc w jasny: {:?}", dark);
        assert!(lum(bright) < 128, "jasny ma pojsc w ciemny: {:?}", bright);
    }
}
