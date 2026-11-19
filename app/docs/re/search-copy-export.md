# Szukanie, kopiowanie, eksport (runda 17)

Trzy rzeczy dla dłuższych sesji: znaleźć w scrollbacku, skopiować wynik
i wynieść transkrypt poza terminal. Wszystko w TUI, bez zmian kontraktów
eventów i MCP oraz bez web UI.

## 1. Wyszukiwanie w scrollbacku — `Ctrl+F`

Modal `Search scrollback` w stylu „Select model” (`welcome::search`):

```
  ~/projects/ArndtAi
┌──────────────────────────────────────────────────────────────────────┐
│  Search scrollback                                              [✗] │
├──────────────────────────────────────────────────────────────────────┤
│ / notatka                                                   1/3     │
│──────────────────────────────────────────────────────────────────────│
│  2  write notatka.txt :: ArndtAi TUI dziala; read notatka.txt        │
│ 11  ◆ Wrote notatka.txt +18 B                                       │
│ 19  ◆ Read notatka.txt +18 B                                        │
│ Esc:back  │  Enter:jump  │  n/N:next/prev  │  ctrl+f:close          │
└──────────────────────────────────────────────────────────────────────┘
```

| klawisz | efekt |
|---|---|
| `Ctrl+F` | otwiera modal (drugie `Ctrl+F` zamyka bez skoku) |
| znaki, `Backspace` | zapytanie; lista i licznik zmieniają się na bieżąco |
| `Enter` | zamyka modal, zostawia zaznaczone trafienie w scrollbacku i oddaje `n`/`N` stopce |
| `n` / `N`, `Ctrl+N` / `Ctrl+P` | następne / poprzednie trafienie |
| `↑` / `↓` | to samo co `N` / `n` |
| `Esc` | zamyka modal i wraca na przewinięcie, z którego przyszedł |

Zasady:

- Trafienia liczone są po **narysowanych wierszach** scrollbacku (po zawijaniu).
  Numer w liście to numer wiersza 1-based, a tekst to początek tego wiersza.
- Puste zapytanie: sam pasek `/ szukaj`, bez listy — to nie błąd.
- Brak trafień: `brak trafien` w modalu i licznik `0/0`.
- Podświetlenie: dopasowanie dostaje `Modifier::REVERSED` (odwrócone kolory),
  paleta i reszta wiersza bez zmian. Zaznaczone trafienie ma dodatkowo pasek
  `bg_light`, jak wiersz w pickerze modeli.
- `n`/`N` działają po `Enter`, bo w trybie pisania `n` jest literą frazy
  (np. „notatka”); dlatego są też `Ctrl+N`/`Ctrl+P`. Po `Enter` modal znika,
  trafienia są podświetlone w scrollbacku, a stopka pokazuje
  `szukanie: <fraza> 2/3 n/N: nastepne Esc: koniec`. `Ctrl+F` otwiera modal
  z powrotem (można dopisać frazę).
- Rozmiar okna może się zmienić w trakcie: trafienia liczone są od nowa po
  szerokości z ostatniej klatki, więc lista nie zostaje „sprzed resize’u”.

Kod: `tui.rs` (`Search`, `open_search`, `search_hits`, `on_key_search`,
`draw_search`, `highlight_spans`), układ modala: `welcome::search`.

## 2. Kopiowanie — `/copy`

```
/copy        # ostatni blok odpowiedzi asystenta (tresc `finish` z podsumowania)
/copy all    # cala widoczna rozmowa
```

- Tekst idzie do schowka przez **OSC 52**: `ESC ] 52 ; c ; <base64> BEL` na
  stdout (`src/clipboard.rs`). Działa przez SSH i w tmuxie, bez `xclip`,
  `wl-copy` ani schowka X11.
- Kopiowany blok jest czysty: bez ramek, bez `◆`/`❯`, bez numerów kroków.
- `/copy all` pisze linie `ty: <zadanie>`, `asystent: <myśl/odpowiedź>`,
  `wynik: <narzędzie> <ścieżka>`, `odmowa: ...`, `system: ...`.
- Potwierdzenie w scrollbacku: `skopiowano N znakow (OSC 52)`.
- Gdy terminal nie przyjmie sekwencji (brak TTY, `TERM=dumb`) albo zapis się
  nie uda: `schowek moze nie zadzialac w tym terminalu` — bez crasha.
- Brak odpowiedzi asystenta w sesji: `brak odpowiedzi asystenta do skopiowania`.

Ograniczenia OSC 52 (terminal, nie program):

- Terminal musi obsługiwać OSC 52 i mieć włączone kopiowanie do schowka
  systemowego. Gnome Terminal, kitty, WezTerm, iTerm2 i Windows Terminal —
  tak; część terminali wymaga włączonej opcji (np. `clipboard` w tmux,
  `set-clipboard on`).
- Nie ma potwierdzenia zwrotnego: terminal milczy także wtedy, gdy schowek
  odrzuci zbyt duży tekst. Bezpieczny rozmiar to zwykle kilkadziesiąt KB;
  długi `/copy all` może zostać obcięty **przez terminal**, nie przez ArndtAi.
- tmux wymaga `set -g set-clipboard on` i terminala obsługującego OSC 52.
- SSH: sekwencja przechodzi przez połączenie, ale schowek trafia do terminala
  po stronie klienta, nie do serwera.
- Multiplexer ekranu może przechwycić sekwencję — wtedy schowek zostaje pusty,
  a komunikat nadal mówi `skopiowano`, bo zapis do terminala się udał.

## 3. Eksport — `/export [plik]`

- Bez argumentu: `eksport-YYYYMMDD-HHMM.md` w workspace (czas lokalny).
- Z argumentem: ścieżka **względna w workspace**, te same bramki co `fs.write`
  (`tools::resolve`): bez `..`, bez ścieżek absolutnych, bez `.agent`.
  Odmowa daje wpis `eksport odrzucony: <plik> — <powód>` i nie tworzy pliku.
- Eksport czyta tylko wpisy TUI; **nie modyfikuje** transkryptu JSONL ani
  bazy SQLite (żadnego `Session::event` po drodze).

Format:

```markdown
# <zadanie>

Data: 2026-11-14 02:31
Model: mimo-v2.6-flash

## krok 1

mysl: najpierw zapis
wywolanie: fs.write a.txt
wynik: a.txt +4 B · alfa
status: ok

## krok 2

wywolanie: fs.read a.txt
wynik: a.txt +4 B · alfa
status: ok

## status koncowy

DONE · kroki 2 · narzedzia 2 · odmowy 0 · dryf 0
```

- Nagłówek `# <zadanie>` bierze pierwszy wpis `task` sesji (albo tytuł
  ostatniej sesji); `Data:` i `Model:` stoją pod nim.
- Bloki `## krok N` powstają z numerów kroków obecnych w scrollbacku.
- W kroku: `mysl:` (treść myśli), `wywolanie: <narzędzie> <ścieżka>`,
  `wynik:` (skrócony do 400 znaków) i `status: ok|blad|odmowa`.
- Na końcu `## status koncowy` z realnymi licznikami tury.

Kod: `tui.rs` (`export_markdown`, `export`).

## Poprawki z testu na żywo (r16)

1. **Data w prompcie.** `provider::system_prompt` dokłada linię
   `Data: YYYY-MM-DD (UTC±HH:MM)` z `src/clock.rs` (czas lokalny, strefa
   z `date +%z`). Model nie ma zegara, więc bez tego raporty dostawały
   wymyśloną datę.
2. **`skill.load` raportuje `0 bajtow`.** Klient MCP czytał rozmiar tylko
   z `size`/`bytes_written`; wynik `skill.load` niesie go w `bytes`
   (`structuredContent.bytes`). `mcp::bytes_from_output` czyta teraz wszystkie
   trzy pola, więc `[OBSERVE] skill.load ok=true (331 bajtow)` pokazuje
   rzeczywisty rozmiar treści.

## Testy i zrzuty

| co | gdzie |
|---|---|
| licznik, `n`/`N`, `Esc` wraca | `tui::tests::ctrl_f_counts_hits_and_n_walks_them` |
| brak trafień i puste zapytanie | `tui::tests::search_without_hits_says_brak_trafien` |
| podświetlenie bez zmiany palety | `tui::tests::highlight_reverses_only_the_match` |
| OSC 52 z oczekiwaną treścią | `clipboard::tests::osc52_sequence_has_the_expected_text_on_a_fake_terminal` |
| `/copy` bez odpowiedzi | `tui::tests::copy_takes_the_last_assistant_block_through_osc52` |
| eksport: nagłówek, kroki, status | `tui::tests::export_writes_header_steps_and_final_status` |
| eksport: `..` → odmowa | `tui::tests::export_refuses_a_path_with_parent_dir` |
| eksport: wynik ≤ 400 znaków | `tui::tests::export_result_is_cut_to_400_chars` |
| data w prompcie | `provider::tests::prompt_carries_todays_local_date_and_zone` |
| bajty `skill.load` przez MCP | `mcp::tests::skill_load_bytes_are_read_from_the_result_envelope` |

Zrzuty 960×608 (`app/shots/`):

- `r17-search.png` — modal szukania z trzema trafieniami i licznikiem `1/3`,
- `r17-copy.png` — scrollback po `/copy` z `skopiowano N znakow (OSC 52)`,
- `r17-export.png` — scrollback po `/export` z nazwą pliku i liczbą kroków.

Flagi: `--screenshot-search`, `--screenshot-copy`, `--screenshot-export`
(ten sam tryb bezobsługowy co `--screenshot`: 120 klatek do bufora, PNG
960×608, bez dotykania terminala).
