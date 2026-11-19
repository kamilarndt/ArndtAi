//! Testy renderera: wejście (markdown) → wyjście (tekst wierszy + style).
//! Każdy element z listy zlecenia ma tu własny przypadek; do tego niepełny
//! markdown, obcinanie kodu, szerokość 40 kolumn i pamięć renderu.

use super::*;

fn texts(lines: &[Line]) -> Vec<String> {
    lines.iter().map(|line| line.text()).collect()
}

fn joined(lines: &[Line]) -> String {
    texts(lines).join("\n")
}

/// Kolory wszystkich spanów w bloku — po nich sprawdzamy podświetlanie.
fn colors(lines: &[Line]) -> Vec<Color> {
    lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .filter_map(|span| span.style.fg)
        .collect()
}

/// Kolor konkretnego fragmentu tekstu (pierwsze trafienie).
fn color_of(lines: &[Line], needle: &str) -> Option<Color> {
    for line in lines {
        for span in &line.spans {
            if span.content == needle {
                return span.style.fg;
            }
        }
    }
    None
}

// ── Nagłówki ────────────────────────────────────────────────────────────────

#[test]
fn headings_drop_the_hashes_and_stay_bold() {
    let lines = render("# Tytul\n\n## Rozdzial\n\n### Podpunkt", 60);
    assert_eq!(texts(&lines), vec!["Tytul", "", "Rozdzial", "", "Podpunkt"]);
    for index in [0, 2, 4] {
        let span = &lines[index].spans[0];
        assert!(
            span.style.add_modifier.contains(Modifier::BOLD),
            "naglowek ma byc pogrubiony: {:?}",
            span.content
        );
        assert!(
            !lines[index].text().contains('#'),
            "znaki # nie moga zostac: {}",
            lines[index].text()
        );
    }
}

#[test]
fn hashes_without_a_space_are_not_a_heading() {
    // `#hashtag` to zwykly tekst, nie naglowek.
    let lines = render("#hashtag", 40);
    assert_eq!(texts(&lines), vec!["#hashtag"]);
}

// ── Pogrubienie i kursywa ───────────────────────────────────────────────────

#[test]
fn bold_and_italic_become_attributes_not_characters() {
    let lines = render("To **wazne** i *lekkie* zdanie", 60);
    assert_eq!(texts(&lines), vec!["To wazne i lekkie zdanie"]);
    let shy = lines[0]
        .spans
        .iter()
        .find(|span| span.content == "wazne")
        .expect("pogrubiony fragment");
    assert!(shy.style.add_modifier.contains(Modifier::BOLD));
    let light = lines[0]
        .spans
        .iter()
        .find(|span| span.content == "lekkie")
        .expect("kursywa");
    assert!(light.style.add_modifier.contains(Modifier::ITALIC));
    assert!(!lines[0].text().contains('*'));
}

#[test]
fn underscore_italic_does_not_break_file_names() {
    // `snake_case` nie moze stac sie kursywa.
    let lines = render("plik state_db_recovery.rs zostaje", 60);
    assert_eq!(texts(&lines), vec!["plik state_db_recovery.rs zostaje"]);
    let lines = render("to _jest_ kursywa", 60);
    let span = lines[0]
        .spans
        .iter()
        .find(|span| span.content == "jest")
        .expect("kursywa");
    assert!(span.style.add_modifier.contains(Modifier::ITALIC));
}

// ── Kod inline ──────────────────────────────────────────────────────────────

#[test]
fn inline_code_gets_its_own_background() {
    let lines = render("uruchom `cargo test` w app", 60);
    assert_eq!(texts(&lines), vec!["uruchom cargo test w app"]);
    let span = lines[0]
        .spans
        .iter()
        .find(|span| span.content == "cargo test")
        .expect("kod inline");
    assert_eq!(span.style.bg, Some(BG_LIGHT), "kod inline ma inne tlo");
    assert_eq!(span.style.fg, Some(FG));
    let plain = lines[0]
        .spans
        .iter()
        .find(|span| span.content.contains("uruchom"))
        .expect("tekst obok");
    assert_eq!(plain.style.bg, None, "reszta bez tla kodu");
}

#[test]
fn bold_inside_inline_code_stays_literal() {
    let lines = render("`**nie bold**`", 40);
    assert_eq!(texts(&lines), vec!["**nie bold**"]);
    assert!(!lines[0].spans[0]
        .style
        .add_modifier
        .contains(Modifier::BOLD));
}

// ── Listy ───────────────────────────────────────────────────────────────────

#[test]
fn lists_are_indented_and_bulleted() {
    let source = "- pierwszy\n- drugi\n  - glebiej\n1. numerowany";
    let lines = render(source, 40);
    assert_eq!(
        texts(&lines),
        vec![
            format!("{} pierwszy", BULLET),
            format!("{} drugi", BULLET),
            format!("  {} glebiej", BULLET),
            "1. numerowany".to_string(),
        ]
    );
}

#[test]
fn a_long_list_item_wraps_under_its_text() {
    let lines = render("- ala ma kota a kot ma ale i to jest dlugi punkt listy", 24);
    assert!(lines.len() > 1, "punkt lamie sie na kilka wierszy");
    for line in &lines {
        assert!(width_of(&line.text()) <= 24, "szerokosc: {:?}", line.text());
    }
    assert!(lines[0].text().starts_with(&format!("{} ", BULLET)));
    assert!(
        lines[1].text().starts_with("  ") && !lines[1].text().starts_with(&format!("{} ", BULLET)),
        "kontynuacja pod tekstem: {:?}",
        lines[1].text()
    );
}

// ── Bloki kodu ──────────────────────────────────────────────────────────────

#[test]
fn code_block_draws_a_frame_with_the_language_label() {
    let source = "```rust\nfn main() {}\n```";
    let lines = render(source, 40);
    let top = lines[0].text();
    assert!(top.starts_with('\u{250C}'), "lewy gorny naroznik: {top:?}");
    assert!(top.ends_with('\u{2510}'), "prawy gorny naroznik: {top:?}");
    assert!(top.contains(" rust "), "etykieta jezyka: {top:?}");
    assert_eq!(width_of(&top), 40, "ramka pelnej szerokosci");
    assert!(
        top.trim_end_matches('\u{2510}').ends_with("rust "),
        "etykieta przy prawym narozniku: {top:?}"
    );
    let code = lines[1].text();
    assert!(code.starts_with("\u{2502} "), "lewa krawedz: {code:?}");
    assert!(code.ends_with(" \u{2502}"), "prawa krawedz: {code:?}");
    assert!(code.contains("fn main() {}"));
    let bottom = lines.last().unwrap().text();
    assert!(bottom.starts_with('\u{2514}') && bottom.ends_with('\u{2518}'));
}

#[test]
fn rust_code_is_highlighted_from_the_palette() {
    let source = "```rust\n// komentarz\nfn main() { let x = 42; }\nlet s = \"tekst\";\n```";
    let lines = render(source, 60);
    assert_eq!(color_of(&lines, "fn"), Some(KEYWORD), "slowo kluczowe");
    assert_eq!(color_of(&lines, "let"), Some(KEYWORD));
    assert_eq!(color_of(&lines, "42"), Some(NUMBER), "liczba");
    assert_eq!(color_of(&lines, "\"tekst\""), Some(STRING), "string");
    let comment = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .find(|span| span.content.starts_with("// komentarz"))
        .expect("komentarz");
    assert_eq!(comment.style.fg, Some(COMMENT), "komentarz przygaszony");
}

#[test]
fn json_bash_and_toml_are_highlighted_too() {
    let json = render("```json\n{ \"k\": 12, \"b\": true }\n```", 60);
    assert_eq!(color_of(&json, "\"k\""), Some(STRING));
    assert_eq!(color_of(&json, "12"), Some(NUMBER));
    assert_eq!(color_of(&json, "true"), Some(KEYWORD));

    let bash = render("```bash\n# komentarz\nif test -f a.txt; then echo \"ok\"; fi\n```", 60);
    assert_eq!(color_of(&bash, "if"), Some(KEYWORD));
    assert_eq!(color_of(&bash, "\"ok\""), Some(STRING));

    let toml = render("```toml\n[agent]\nkroki = 12 # limit\n```", 60);
    assert_eq!(color_of(&toml, "[agent]"), Some(KEYWORD));
    assert_eq!(color_of(&toml, "12"), Some(NUMBER));
}

#[test]
fn other_languages_get_no_highlighting() {
    let lines = render("```python\ndef x(a):\n    return \"t\"\n```", 60);
    let colors = colors(&lines);
    // Ramka ma wlasny kolor; tresc kodu — jedna barwa.
    let body_colors: Vec<Color> = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .filter(|span| {
            !span.content.trim().is_empty()
                && !span.content.contains('\u{2502}')
                && !span.content.contains('\u{250C}')
                && !span.content.contains('\u{2514}')
                && !span.content.contains('\u{2500}')
        })
        .filter_map(|span| span.style.fg)
        .collect();
    assert!(
        body_colors.iter().all(|color| *color == FG),
        "bez podswietlania: {body_colors:?} / ramka {colors:?}"
    );
}

#[test]
fn long_code_lines_are_cut_with_a_count() {
    let long = "x".repeat(120);
    let source = format!("```rust\n{}\n```", long);
    let lines = render(&source, 40);
    let code = lines[1].text();
    assert!(code.contains('\u{2026}'), "wielokropek przy cieciu: {code:?}");
    assert!(code.contains("znakow"), "informacja o cieciu: {code:?}");
    // Ukryte znaki: 120 minus to, co widac.
    let hidden: usize = code
        .split('+')
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .expect("liczba ukrytych znakow");
    let visible = code.matches('x').count();
    assert_eq!(visible + hidden, 120, "suma widocznych i ukrytych znakow");
    assert_eq!(width_of(&code), 40, "wiersz kodu nie wychodzi z ramki");
}

#[test]
fn a_code_block_over_200_lines_is_truncated() {
    let mut body = String::from("```bash\n");
    for i in 0..205 {
        body.push_str(&format!("echo {}\n", i));
    }
    body.push_str("```");
    let lines = render(&body, 60);
    let text = joined(&lines);
    assert!(text.contains("\u{2026} (5 wiecej)"), "skrot: {text}");
    let code_lines = lines
        .iter()
        .filter(|line| line.text().starts_with("\u{2502} "))
        .count();
    assert_eq!(code_lines, CODE_MAX_LINES + 1, "200 linii + wiersz skrotu");
}

#[test]
fn an_unclosed_fence_swallows_the_rest_without_panicking() {
    let lines = render("```rust\nfn main() {}", 40);
    assert!(joined(&lines).contains("fn main() {}"));
    assert!(joined(&lines).contains('\u{2514}'), "ramka zostaje domknieta");
}

// ── Cytat ───────────────────────────────────────────────────────────────────

#[test]
fn blockquote_has_a_bar_and_dimmed_text() {
    let lines = render("> zdanie cytatu", 40);
    assert_eq!(lines[0].text(), "\u{2502} zdanie cytatu");
    let bar = &lines[0].spans[0];
    assert_eq!(bar.style.fg, Some(GRAY_DIM), "kreska z lewej");
    let text = lines[0]
        .spans
        .iter()
        .find(|span| span.content.contains("cytatu"))
        .expect("tekst cytatu");
    assert_eq!(text.style.fg, Some(GRAY), "tekst przygaszony");
}

#[test]
fn nested_blockquote_repeats_the_bar() {
    let lines = render(">> glebiej", 40);
    assert_eq!(lines[0].text(), "\u{2502} \u{2502} glebiej");
}

// ── Tabela ──────────────────────────────────────────────────────────────────

#[test]
fn table_draws_a_simple_grid() {
    let source = "| Krok | Wynik |\n|---|---|\n| 1 | ok |\n| 2 | blad |";
    let lines = render(source, 60);
    let texts = texts(&lines);
    assert_eq!(texts.len(), 4, "naglowek + kreska + dwa wiersze: {texts:?}");
    assert!(texts[0].starts_with('\u{2502}') && texts[0].ends_with('\u{2502}'));
    assert!(texts[0].contains("Krok") && texts[0].contains("Wynik"));
    assert!(texts[1].contains('\u{253C}'), "przelot w kresce: {:?}", texts[1]);
    assert!(texts[2].contains('1') && texts[2].contains("ok"));
    assert!(texts[3].contains("blad"));
    // Kolumny nie zawijaja tresci: kazdy wiersz danych ma trzy kreski.
    for line in [&texts[0], &texts[2], &texts[3]] {
        assert_eq!(line.matches('\u{2502}').count(), 3, "trzy kreski: {line:?}");
    }
}

#[test]
fn table_cells_stay_inside_the_terminal() {
    let source = "| Kontrola | Wynik dla calego przebiegu |\n|---|---|\n| kroki | trzydziesci dziewiec z czterdziestu |";
    let lines = render(source, 40);
    for line in &lines {
        assert!(width_of(&line.text()) <= 40, "szerokosc: {:?}", line.text());
    }
    assert!(joined(&lines).contains('\u{2026}'), "dluga komorka jest ucinana");
}

// ── Linki i obrazy ──────────────────────────────────────────────────────────

#[test]
fn a_link_is_underlined_text_plus_the_address() {
    let lines = render("opis [dokumentacja](https://example.test/x) tutaj", 60);
    assert_eq!(
        lines[0].text(),
        "opis dokumentacja (https://example.test/x) tutaj"
    );
    let span = lines[0]
        .spans
        .iter()
        .find(|span| span.content == "dokumentacja")
        .expect("tekst linku");
    assert!(
        span.style.add_modifier.contains(Modifier::UNDERLINED),
        "link jest podkreslony"
    );
}

#[test]
fn an_image_is_its_caption_and_address_not_a_picture() {
    let lines = render("![zrzut ekranu](shot.png)", 60);
    assert_eq!(lines[0].text(), "zrzut ekranu (shot.png)");
    assert!(!lines[0].text().contains("!["));
}

// ── Zawijanie i szerokość ───────────────────────────────────────────────────

#[test]
fn text_wraps_at_the_terminal_width() {
    let source = "ala ma kota a kot ma ale i wszystko miesci sie w waskich kolumnach";
    let lines = render(source, 20);
    assert!(lines.len() > 1);
    for line in &lines {
        assert!(width_of(&line.text()) <= 20, "szerokosc: {:?}", line.text());
        assert!(!line.text().starts_with(' '), "bez wciecia na starcie");
    }
    assert!(joined(&lines).replace('\n', " ").contains("waskich kolumnach"));
}

/// Pełny dokument z każdego elementu — przy 40 kolumnach nic nie wychodzi
/// za szerokość i nic się nie nachodzi (siatka wierszy).
#[test]
fn forty_columns_keeps_every_line_inside_the_screen() {
    let source = "\
# Raport

Akapit z **pogrubieniem**, *kursywa* i `kodem`, ktory sie zawija.

- punkt pierwszy z dlugim opisem, ktory musi sie zawinac
- punkt drugi
  1. numerowany w srodku

> cytat, ktory tez sie zawija na kilka wierszy

```rust
fn main() {
    let nazwa = \"bardzo dlugi napis, ktory nie miesci sie w czterdziestu kolumnach\";
}
```

| Krok | Wynik |
|---|---|
| 1 | ok |

---";
    for width in [40usize, 39, 30, 20, 12] {
        let lines = render(source, width);
        let limit = width.max(8);
        for line in &lines {
            assert!(
                width_of(&line.text()) <= limit,
                "w={width}: {:?} ({})",
                line.text(),
                width_of(&line.text())
            );
        }
        // Poza kodem ramki nie moga wystapic prawa krawedz przy 40 kolumnach.
        assert!(!lines.is_empty());
    }
}

#[test]
fn narrow_code_frames_drop_the_right_edge() {
    let source = "```rust\nfn main() {}\n```";
    let wide = render(source, 60);
    assert!(wide[0].text().ends_with('\u{2510}'));
    let narrow = render(source, 30);
    let top = narrow[0].text();
    assert!(!top.contains('\u{2510}'), "bez prawej krawedzi: {top:?}");
    assert!(!top.contains('\u{250C}'), "bez lewego naroznika: {top:?}");
    assert!(top.contains("rust"), "etykieta zostaje: {top:?}");
    assert_eq!(width_of(&top), 30);
    let code = narrow[1].text();
    assert!(code.starts_with("\u{2502} "));
    assert!(!code.ends_with('\u{2502}'), "bez prawej krawedzi: {code:?}");
    assert!(joined(&narrow).contains("fn main() {}"), "tresc zostaje");
    for line in &narrow {
        assert!(width_of(&line.text()) <= 30, "{:?}", line.text());
    }
}

// ── Niepełny markdown ───────────────────────────────────────────────────────

#[test]
fn incomplete_markdown_never_breaks_the_render() {
    let cases = [
        "**bez konca",
        "*jedna gwiazdka",
        "_podkreslenie",
        "`niezamkniety kod",
        "[link bez adresu](",
        "![obraz bez adresu](",
        "```",
        "```rust",
        "#",
        "| a | b",
        ">",
        "-",
        "1.",
        "\u{2014}\u{2014}",
    ];
    for case in cases {
        let lines = render(case, 40);
        assert!(lines.len() <= 4, "{case:?} → za duzo wierszy: {:?}", texts(&lines));
        for line in &lines {
            assert!(width_of(&line.text()) <= 40, "{case:?}: {:?}", line.text());
        }
    }
    // Niezamknięte znaczniki zostają znakami — nic nie znika.
    assert_eq!(texts(&render("**bez konca", 40)), vec!["**bez konca"]);
    assert_eq!(texts(&render("`niezamkniety kod", 40)), vec!["`niezamkniety kod"]);
}

/// Każdy prefiks dokumentu (tak wygląda odpowiedź w trakcie pisania) i każda
/// szerokość: bez paniki i bez wiersza szerszego od ekranu. Prefiksy łamią
/// znaczniki w środku, także w środku znaku UTF-8.
#[test]
fn every_prefix_of_a_streaming_answer_renders() {
    let source = "## Plan i `kod`\n\nAkapit z **pogrubieniem**, *kursywa* i [linkiem](u).\n\n- punkt\n  - głębiej\n1. numer\n\n```rust\nfn main() { let x = 3; } // komentarz\n```\n\n| Krok | Wynik |\n|---|---|\n| 1 | ok |\n\n> cytat żółć\n\n---";
    let mut cut = 0;
    while cut <= source.len() {
        if source.is_char_boundary(cut) {
            let prefix = &source[..cut];
            for width in [8usize, 12, 20, 40, 80, 120] {
                let lines = render(prefix, width);
                for line in &lines {
                    assert!(
                        width_of(&line.text()) <= width,
                        "prefiks {cut} / w={width}: {:?}",
                        line.text()
                    );
                }
            }
        }
        cut += 1;
    }
}

// ── Pamięć renderu (streaming) ──────────────────────────────────────────────

#[test]
fn the_same_block_is_parsed_once_per_width() {
    let cache = Cache::new();
    let text = "## Plan\n\n- punkt";
    let first = cache.get(text, 80);
    let second = cache.get(text, 80);
    assert_eq!(cache.parses(), 1, "drugie rysowanie bierze z pamieci");
    assert_eq!(cache.hits(), 1);
    assert_eq!(texts(&first), texts(&second));
    cache.get(text, 40);
    assert_eq!(cache.parses(), 2, "inna szerokosc to inny uklad");
}

#[test]
fn streaming_reparses_only_the_block_that_grew() {
    let cache = Cache::new();
    let first = cache.get("## Plan\n\n- punkt pierwszy", 80);
    let grown = format!("{}\n- punkt drugi", "## Plan\n\n- punkt pierwszy");
    let second = cache.get(&grown, 80);
    assert_eq!(cache.parses(), 2, "rosnacy blok parsujemy raz");
    assert!(second.len() > first.len(), "blok rosnie w miejscu");
    // Kolejna klatka: oba bloki z pamieci.
    cache.get("## Plan\n\n- punkt pierwszy", 80);
    cache.get(&grown, 80);
    assert_eq!(cache.parses(), 2, "klatki bez zmian nie parsuja nic");
    assert_eq!(cache.hits(), 2);
}

// ── Koszt renderu ───────────────────────────────────────────────────────────

/// Koszt jednego bloku: próbka z nagłówkiem, listą, kodem, tabelą i cytatem.
/// Liczby idą do `app/docs/re/markdown.md`; próg jest luźny (build debug),
/// bo test pilnuje, że render nie zamienia się w sekundę na klatkę.
#[test]
fn a_typical_answer_renders_quickly() {
    let source = "\n\n".to_string() + &std::iter::repeat(PROBE).take(10).collect::<Vec<_>>().join("\n\n");
    let started = std::time::Instant::now();
    let mut lines = 0;
    for _ in 0..20 {
        lines += render(&source, 120).len();
    }
    let each = started.elapsed() / 20;
    println!(
        "render bloku ({} znakow, {} wierszy): {:?}",
        source.chars().count(),
        lines / 20,
        each
    );
    assert!(
        each < std::time::Duration::from_millis(50),
        "render jest za wolny na klatke: {:?}",
        each
    );
}

const PROBE: &str = "## Plan przebiegu

Zapisuje `notatka.txt`, potem czytam plik i porownuje **kroki** oraz *wynik*.

1. zapis pliku
2. odczyt i sprawdzenie
3. porownanie krokow

```rust
fn main() {
    let kroki = 3; // limit
    println!(\"kroki: {}\", kroki);
}
```

| Krok | Wynik | Uwaga |
|---|---|---|
| 1 | ok | plik zapisany |
| 2 | ok | tresc zgodna |

> `..` poza workspace to odmowa.";

// ── Ramka tabeli z nagłówkiem ───────────────────────────────────────────────

#[test]
fn table_header_is_bold_and_body_is_plain() {
    let lines = render("| Krok | Wynik |\n|---|---|\n| 1 | ok |", 60);
    let header_bold = lines[0]
        .spans
        .iter()
        .any(|span| span.style.add_modifier.contains(Modifier::BOLD));
    assert!(header_bold, "naglowek tabeli pogrubiony: {:?}", texts(&lines));
    let body_bold = lines[2]
        .spans
        .iter()
        .any(|span| span.style.add_modifier.contains(Modifier::BOLD));
    assert!(!body_bold, "tresc tabeli bez bolda");
}
