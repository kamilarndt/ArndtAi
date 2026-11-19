# Własna ścieżka po podmianie binarki (runda 13)

Data: 2026-11-14. Zakres: `app/src/mcp.rs` (rozwiązywanie ścieżki do siebie),
`app/src/tui.rs` (powrót tekstu do composera), `app/tests/cli.rs` (podmiana
binarki). Protokół MCP, kontrakty eventów, bramki, allowlista, dryf i wygląd
TUI — bez zmian.

## 1. Skąd bierze się ścieżka

Perimetr MCP to **osobny proces**: klient w pętli uruchamia `ardntai mcp
--workspace FOLDER`, więc potrzebuje ścieżki do własnego pliku. Do tej pory
było to jedno wywołanie `std::env::current_exe()` w `McpClient::spawn`.

Na Linuksie ta ścieżka pochodzi z dowiązania `/proc/self/exe`. Gdy plik
binarki zostanie **podmieniony w trakcie działania procesu** (`cargo build`
robi dokładnie `rm` + zapis nowego pliku), jądro dokleja do ścieżki sufiks
`" (deleted)"`, bo stary i-węzeł nie ma już nazwy w katalogu. To nie jest
prawdziwa ścieżka — `Command::new` dostawał nieistniejący plik i każde
kolejne zadanie kończyło się `FAILED_ERROR` z komunikatem
`No such file or directory (os error 2)`.

Dowód mechanizmu (proces z podmienionym plikiem, nowy plik leży pod tą samą
nazwą):

```
przed: /scratch/probe13d/ardntai
po:    /scratch/probe13d/ardntai (deleted)
nowy plik istnieje: tak
```

## 2. Kolejność fallbacków

`mcp::resolve_from(exe, ARNDTAI_SELF, katalogi PATH)` — jedno miejsce,
używane przez `McpClient::spawn`:

1. `current_exe()` bez zmian, jeśli plik istnieje.
2. `current_exe()` z odciętym sufiksem `" (deleted)"`, jeśli plik pod tą
   ścieżką istnieje. To naprawia zwykłą podmiankę `cargo build`: nowy plik
   leży pod tą samą nazwą, więc wystarczy poprawić ścieżkę.
3. `ARNDTAI_SELF` — ścieżka podana przez użytkownika, gdy stara lokalizacja
   naprawdę zniknęła (np. binarka uruchomiona z katalogu, który został
   usunięty).
4. `ardntai` w katalogach z `PATH`.

Pierwszy **istniejący** plik wygrywa; samo „wygląda jak ścieżka" nie
wystarcza.

Gdy zawiodą wszystkie cztery, błąd mówi, czego próbowano i dlaczego:

```
nie mozna ustalic wlasnej sciezki; binarka zostala podmieniona w trakcie
dzialania procesu; sprawdzilem: /x/ardntai, /y (ARNDTAI_SELF), PATH: brak ardtai
```

`spawn` dokłada własny komunikat, gdy plik zniknie **między** sprawdzeniem
a `exec` (`ErrorKind::NotFound`): podaje ścieżkę, której użył, i zdanie
o podmianie binarki oraz podpowiedź `ARNDTAI_SELF`.

## 3. Powrót tekstu do composera

Po statusie `FAILED*` (m.in. `FAILED_ERROR`, `FAILED_BUDGET`,
`FAILED_STALLED`) zadanie wraca do pola wpisywania z kursorem na końcu —
nie trzeba pisać go od nowa. Ustalenia:

- wraca **ostatnio wysłane** zadanie, zapamiętane przy `Enter`;
- jeśli użytkownik zdążył coś wpisać po wysłaniu, jego nowszy tekst zostaje
  nietknięty (stare zadanie go nie nadpisuje);
- po `DONE` composer zostaje pusty, jak dotychczas;
- błąd nadal jest w scrollbacku (event `error` z pętli) i w pasku stanu;
- licznik `kroki` nie rośnie — nieudana wysyłka nie jest krokiem.

## 4. Czego nie da się naprawić

Proces **nie wymieni własnego obrazu w pamięci**. Po podmianie pliku działa
dalej stary kod aż do końca życia procesu; nowy plik jest używany tylko do
uruchamiania nowych procesów (perimetr). Dlatego naprawa dotyczy wyłącznie
ścieżki do spawnu, a nie „przeładowania" binarki. Jeśli plik zniknie i nie
ma żadnego z fallbacków, proces nadal będzie żył, ale perimetru nie odzyska —
wtedy jedynym wyjściem jest restart TUI.

Nie zmieniamy też nazwy pliku ani nie tworzymy kopii „na zapas" przy starcie:
to maskowałoby problem i zostawiało stare binarki w workspace.

## 5. Dowody

- `cargo build` — exit 0, bez ostrzeżeń.
- `cargo test` — 124 testy jednostkowe i 9 integracyjnych, wszystkie zielone.
  Nowe jednostkowe: `strip_deleted_suffix_drops_linux_marker_only`,
  `replaced_binary_falls_back_to_self_env`, `missing_binary_falls_back_to_path`,
  `all_fallbacks_missing_reports_paths_and_reason`,
  `existing_exe_wins_over_env`; w TUI:
  `failed_turn_gives_the_task_back_to_the_composer`,
  `finished_turn_leaves_the_composer_empty`, `failed_turn_keeps_newer_typing`.
  Integracyjne (`tests/cli.rs`): `replaced_binary_still_has_a_perimeter`
  (kopia binarki → `rm` + `cp` → kolejne zadanie w tej samej sesji dalej ma
  perimetr) oraz `deleted_binary_falls_back_to_self_env`.
- `cargo run -- demo` — 39/39.
- Na żywo, prawdziwy terminal (pty): TUI wystartowało, plik binarki
  podmieniono (`rm` + `cp`), zadanie wpisane **po** podmianie wykonało się
  (`write live.txt :: zywy` → plik 4 B, transkrypt `"status":"DONE"`,
  zero `FAILED_ERROR`). Wariant B: stara ścieżka usunięta na dobre,
  `ARNDTAI_SELF` wskazywał nową — zadanie też przeszło (`fallback.txt`),
  zero `FAILED_ERROR`. Wariant C: binarka usunięta, brak fallbacków —
  trwała klatka TUI pokazuje w scrollbacku `✗ perimeter MCP nie wystartowal:
  ... binarka zostala podmieniona w trakcie dzialania procesu; sprawdzilem:
  <ścieżka>, ARNDTAI_SELF: nie ustawiona, PATH: brak ardtai`, pasek stanu
  `FAILED_ERROR`, a w composerze odzyskany tekst `❯ write zgub.txt :: tekst`.
- `cargo run -- --screenshot shots/now.png` — PNG 960×608, układ bez zmian
  (górny pasek, scrollback, composer, pasek stanu).
