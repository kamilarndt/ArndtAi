# Runda 20: render markdown w web UI (parytet z TUI)

TUI renderuje markdown w blokach asystenta (`app/src/markdown.rs`, runda 18).
Web UI pokazuje dotąd surowy tekst. Zadanie: ten sam zakres w `app/web/`,
bez nowych zależności i bez CDN.

## Co renderować (dokładnie jak TUI)

- nagłówki `#`–`###` (pogrubione, bez `#`),
- `**pogrubienie**`, `*kursywa*` jako atrybuty (nie znaki),
- kod inline `` `x` `` (inne tło, ta sama paleta),
- listy `-`/`1.` z wcięciem (numerowane zachowują numery),
- bloki ``` z językiem: ramka, etykieta języka, podświetlanie dla `rust`, `json`,
  `bash`, `toml`,
- cytat `>` (pionowa kreska, przygaszony tekst),
- tabela `| a | b |` (siatka, bez zawijania kolumn).

## Czego nie renderować

Obrazów (podpis + adres), klikalnych linków (podkreślony tekst + adres
w nawiasie), HTML, LaTeX, mermaid. Emoji bez zmian.

## Zasady

1. Markdown **tylko** w treści asystenta (`thought`, `summary.reason`).
   Tekst użytkownika (`task`) i wynik narzędzi (`tool_result.output`) zostają
   dosłowne — `**` w pliku użytkownika nie może zniknąć.
2. Niepełny markdown (`**bez końca`, ```` ```bez zamknięcia ````) nie może
   wywalić renderu — pokaż resztę jako tekst.
3. Blok kodu > 200 linii → obcięty z `… (N więcej)`; linia dłuższa niż 120 znaków
   → `… +N znaków`.
4. Renderowanie nie może psuć taila: nowe eventy nadal dopisują się bez
   przeładowania, a niepełna linia nadal czeka w buforze.
5. Zero nowych plików poza `app/web/`; zero zmian w `app/src/`.

## Dowód (odbieram sam)

- `cd app && cargo test --quiet` → 232 + 20, `cargo run --quiet -- demo` → 39/39
  (rdzeń nietknięty).
- `python3 app/web/fixture_events.py --out /tmp/fx-md --markdown` tworzy sesję
  z próbką każdego elementu (nagłówek, pogrubienie, kursywa, kod inline, listy,
  blok `rust` i `json`, cytat, tabela, niepełny markdown); UI pokazuje każdy
  element zgodnie z listą wyżej.
- Zrzut `app/shots/faza-c-markdown.png` (960 px szerokości panelu eventów)
  i ten sam ekran przy 500 px (uproszczenie ramek, treść zostaje).
- `app/web/README.md`: co obsługiwane, co pominięte, jak włączyć próbkę.

## Czego nie ruszać

Kontraktów, `app/src/`, TUI, taila po pełnych liniach, filtra `kind`, szukania.
