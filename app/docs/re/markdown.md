# Markdown w odpowiedziach asystenta (runda 18)

**Data:** 2026-11-14. Kod: `src/markdown.rs` (renderer), `src/tui.rs`
(wstawienie w scrollback), zrzuty: `shots/markdown.png` (120×38 = 960×608),
`shots/markdown-40.png` (40×38).

Zasada: renderer jest osobnym modułem bez stanu. Wejście — surowy tekst
modelu; wyjście — lista wierszy (`Line`: spany + tło), które `tui.rs` rysuje
jak każdy inny wpis scrollbacku. Dzięki temu wygląd bierze się z palety
`groknight`, a motyw `jasny` działa przez `theme::recolor` bez drugiego
zestawu kolorów.

## Co jest obsługiwane

| element | wejście | wyjście |
|---|---|---|
| nagłówki | `# Tytul`, `## Rozdzial`, `### Podpunkt` | pogrubiony tekst bez `#`; `####`+ przygaszony bold |
| pogrubienie | `**x**` | atrybut bold, znaki znikają |
| kursywa | `*x*`, `_x_` | atrybut italic; `_` tylko na granicy słowa, więc `state_db_recovery.rs` zostaje tekstem |
| kod inline | `` `cargo test` `` | ten sam kolor tekstu, tło `bg_light` (w motywie jasnym `#e8e8ec`) |
| listy | `- x`, `* x`, `+ x`, `1. x`, `1) x` | wcięcie 2 znaki na poziom + punktor `•`; listy numerowane zachowują numer |
| blok kodu | ```` ```rust ```` … ```` ``` ```` | ramka `┌─ … ─┐`, etykieta języka przy prawym górnym narożniku, treść w środku |
| podświetlanie | `rust`, `json`, `bash`/`sh`/`zsh`, `toml` | słowa kluczowe — magenta, stringi — zieleń, liczby — pomarańcz, komentarze — `gray_dim` |
| cytat | `> tekst`, `>> głębiej` | pionowa kreska `│` z lewej, tekst w `gray` |
| tabela | `\| a \| b \|` + wiersz `\|---\|---\|` | siatka `│`, pogrubiony nagłówek, kreska `├─┼─┤`, kolumny bez zawijania |
| reguła | `---`, `***`, `___` | `——` przygaszone |
| link | `[tekst](adres)` | tekst podkreślony + ` (adres)` — bez klikania |
| obraz | `![podpis](plik.png)` | sam podpis + ` (plik.png)` — bez obrazka |

Zawijanie: tekst łamie się na szerokości terminala (miękko, na spacjach;
słowo dłuższe niż cała szerokość jest cięte twardo). Blok kodu nigdy nie łamie
się w środku słowa: linia szersza niż wnętrze ramki jest ucinana z `… +N znakow`
albo `… +N` w wąskiej ramce, gdy pełna informacja się nie mieści.

Blok kodu dłuższy niż 200 linii kończy się wierszem `… (N wiecej)`; reszta
linii nie jest liczona ani rysowana.

Szerokość < 40 kolumn: ramka kodu traci prawą krawędź (zostaje `│ ` z lewej,
same myślniki u góry i u dołu, etykieta języka przy prawym końcu górnej
kreski), treść zostaje. Tabele przycinają najszerszą kolumnę wielokropkiem,
żeby zmieścić się w terminalu. Nagłówki nie mają ramki, więc w wąskim oknie
różnią się tylko brakiem `#` i pogrubieniem.

## Czego świadomie nie ma

- **Obrazów** — terminal nie ma czym ich pokazać; zostaje podpis i adres.
- **Klikalnych linków** — OSC 8 wymaga wsparcia terminala i psuje kopiowanie
  tekstu; zostaje tekst podkreślony plus adres w nawiasie.
- **HTML** — idzie dosłownie, tak jak przyszło (żadnego `innerHTML` w TUI).
- **LaTeX / mermaid** — bez renderowania; mermaid to osobna runda, jeśli w ogóle.
- **Emoji zamienianych na ikony** — znaki przechodzą bez podmiany.
- **Pełnego CommonMarka**: bez tabel zagnieżdżonych, bez list w cytatach, bez
  `~~strike~~`, bez odnośników przypisów, bez bloków indykowanych czterema
  spacjami. Model dostaje instrukcję prostego markdownu, a renderer pokrywa
  to, co realnie pisze.

Decyzje, które warto znać:

- **Listy numerowane zachowują numer** (`1.`, `2.`), a nie dostają `•` —
  numer niesie informację, której punktor by nie oddał. Nienumerowane mają `•`.
- **Twarde złamania linii są respektowane**: każda linia źródła to osobny
  akapit (nie sklejamy sąsiednich linii w jeden akapit jak CommonMark), bo
  model formatuje odpowiedzi liniami i użytkownik widzi to samo, co napisał model.
- **Tabela powstaje tylko z wierszem `|---|`**; bez niego `| a | b |` to zwykły
  tekst (unikamy zjadania pionowych kresek w kodzie i w ścieżkach).
- **Markdown obowiązuje wyłącznie w bloku asystenta** (`Kind::Thought`).
  Tekst zadania (`Kind::Task`), wynik narzędzia (`Kind::Ok`, `Kind::Fail`) i
  powód zakończenia sesji (`Kind::Summary`) rysują się dosłownie — `**`, `#`
  i `- ` w pliku użytkownika nie mogą zniknąć.

## Streaming i koszt renderu

Model w tej wersji przysyła jedną myśl na krok (`agent.rs`: `thought` z całym
tekstem), więc strumień tokenów nie istnieje. Renderer jest jednak gotowy na
dopisywanie: gdy przyjdzie `thought` z tym samym krokiem i tekstem zaczynającym
się od poprzedniego, wpis rośnie **w miejscu** (bez nowego bloku i bez
przeskoku scrollbacku) — patrz test `a_streamed_answer_grows_in_place`.

Żeby dopisywanie nie kosztowało pełnego renderu:

1. `markdown::Cache` trzyma wynik per `(tekst, szerokość)`. Rosnący blok
   zmienia jeden klucz; pozostałe bloki są brane z pamięci
   (test `streaming_reparses_only_the_block_that_grew`: 3 klatki bez zmian = 0
   ponownych parsowań). Pamięć jest ograniczona do 64 bloków.
2. `Terminal::draw` w ratatui zapisuje tylko komórki, które się różnią, więc
   ekran nie jest czyszczony przy każdej klatce i nie migocze.

Zmierzone koszty (blok 3 750 znaków → 219 wierszy, próbka z każdego elementu):

| build | czas jednego renderu |
|---|---|
| release | ≈ 0,2 ms |
| debug | ≈ 1,6 ms |

Test `a_typical_answer_renders_quickly` pilnuje górnego progu (50 ms w debug),
więc regresja „sekunda na klatkę" nie przejdzie niezauważona.

## Testy

Renderer (`src/markdown/tests.rs`, 30 przypadków):

- każdy element z tabeli wyżej ma własny test wejście → wyjście tekstowe
  (nagłówki, bold, kursywa, kod inline, listy, blok kodu z etykietą,
  podświetlanie `rust`/`json`/`bash`/`toml` i brak podświetlania dla `python`,
  cytat, tabela, link, obraz, reguła),
- niepełny markdown nie wywraca renderu (`**bez konca`, `` `kod ``, `[link](`,
  ```` ``` ```` bez zamknięcia, `| a | b` bez kreski),
- blok > 200 linii obcinany z `… (N wiecej)`,
- szerokości 40, 39, 30, 20 i 12 kolumn: żaden wiersz nie wychodzi za ekran,
- pamięć renderu: to samo wejście parsowane raz, inna szerokość to inny układ.

W `tui.rs` dochodzą trzy testy integracyjne: markdown tylko w bloku asystenta,
rosnący blok w miejscu, oraz zrzut 120×38 i 40×38 z nagłówkiem, listą, kodem,
tabelą i cytatem.
