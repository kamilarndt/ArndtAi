# Runda 16: skille — `SKILL.md` naprawdę w kontekście

Data: 2026-11-14. Zakres: odkrywanie skilli, nagłówek YAML, lista w promptcie
systemowym, narzędzie `skill.load`, ekran `/skills`, komenda `/skill` i licznik
`skille N` w statusie. Kontrakty (`docs/contracts/`), okno kontekstu,
ustawienia z r15, dryf, bramki zakresu i schemat bazy — bez zmian.

## 1. Format pliku

```
<workspace>/skills/<nazwa>/SKILL.md
```

Nagłówek YAML między liniami `---`, pola: `name` i `description` (wymagane),
`when` (opcjonalne, kiedy używać). Treść to wszystko po zamykającym `---`.

```markdown
---
name: raport-dzienny
description: dzienny raport z postepu prac
when: gdy trzeba podsumowac dzien
---
# Raport dzienny

1. Zbierz wykonane kroki z transkryptu sesji.
2. Podaj trzy liczby: kroki, narzedzia, odmowy.
```

Zasady parsowania (`src/skills.rs::parse_document`):

| przypadek | efekt |
|---|---|
| brak nagłówka (`---`) | `name` = nazwa katalogu, `description` = pierwsza niepusta linia tekstu (bez `#`) |
| brak pliku `SKILL.md` | katalog pomijany przy odkrywaniu |
| nagłówek bez zamknięcia `---` | skill na liście, `błąd nagłówka`, nie wstrzykiwany |
| linia nagłówka bez `klucz: wartość` | j.w. (`linia N naglowka nie jest 'klucz: wartosc'`) |
| brak `name` albo `description` | j.w. — start programu działa normalnie |
| nieznane pole w nagłówku | ignorowane (np. `tags`) |
| wartość w cudzysłowach | zdejmowane (`"..."` / `'...'`) |

Zepsuty nagłówek **nie wywala startu**: skill trafia na listę z oznaczeniem
`! błąd nagłówka` (TUI), `[blad naglowka]` (CLI), ale nie jest wstrzykiwany
ani nie da się go wczytać.

## 2. Kolejność katalogów

1. `<workspace>/skills/` — źródło `workspace`
2. `<katalog aplikacji>/skills/` — źródło `aplikacja` (katalog binarki;
   `ARNDTAI_APP_DIR` pozwala wskazać inny)
3. `~/.config/arndtai/skills/` (albo `$XDG_CONFIG_HOME/arndtai/skills`) —
   źródło `config`

Ten sam `name` w późniejszym katalogu **nie nadpisuje** wcześniejszego:
pierwszeństwo ma workspace. Lista wynikowa jest sortowana po `name`, więc
zrzut i CLI pokazują zawsze tę samą kolejność.

## 3. Co idzie do kontekstu

- **Lista** — do promptu **systemowego** (`provider::system_prompt`), wiersze
  `- <name>: <description>`, maksymalnie 20 pozycji, posortowane po nazwie.
  Zepsute nagłówki nie wchodzą. Sekcja pojawia się tylko wtedy, gdy są skille.
- **Pełna treść** — wyłącznie po `/skill <nazwa>` (TUI) albo przez narzędzie
  `skill.load` (model). Limit 8 KB (`skills::MAX_CONTENT_BYTES`); dłuższa treść
  jest obcinana po granicy znaku, z jawnym ostrzeżeniem
  (`truncated: true`, `warning` w wyniku narzędzia).
- W `Prefix::render` (prompt użytkownika) zostaje sam licznik
  `N skilli — lista w promptcie systemowym` — lista nie jest płacona dwa razy.

## 4. Koszt kontekstu

Lista to jedyny stały koszt; pełna treść wchodzi na żądanie.

| pozycja | znaki (z `\n`) |
|---|---|
| `- notatki: zapisuje notatki i decyzje do pliku` | 47 |
| `- raport-dzienny: dzienny raport z postepu prac` | 48 |
| nagłówek sekcji `Skille (pelna tresc tylko przez skill.load):` | 46 |

Dwa skille = **95 znaków listy**, razem z nagłówkiem sekcji **141 znaków**
(~35 tokenów przy `count/4`). Sufit 20 pozycji to ~1000 znaków przy opisach
po ~50 znaków. Dla porównania: limit 8 KB jednego skilla to ok. 2000 tokenów —
dlatego treść wchodzi tylko na żądanie.

## 5. Narzędzie `skill.load`

Piąta pozycja perimetru (`src/tools.rs`), read-only — nie jest mutacją, więc
nie wchodzi do okna dryfu i nie pyta o zgodę w trybie `ask`. Te same bramki
co reszta narzędzi:

| wejście | wynik |
|---|---|
| `{"name": "raport-dzienny"}` | `{"content": "...", "bytes": N, "truncated": false}` |
| treść > 8 KB | `content` obcięte, `truncated: true`, `warning` |
| nazwa spoza listy | `NOT_FOUND` |
| `../etc/passwd`, `a/b`, `..` | `DENIED` (nazwa nie może być ścieżką) |
| brak `name` | `SCHEMA_ERROR` |
| zepsuty nagłówek | `SCHEMA_ERROR` z powodem |

Kontrakt `docs/contracts/mcp.md` dopisuje autor rundy — tutaj zmiana jest
tylko w kodzie (`tools::NAMES`, `describe`, `input_schema`, `call`).

## 6. TUI

| element | zachowanie |
|---|---|
| `/skills` | modal: `nazwa · opis · rozmiar`, znacznik `✓ w kontekście`, `●` zaznaczenie |
| `↑`/`↓` | wybór wiersza |
| `Enter` | podgląd treści w scrollbacku (modal się zamyka) |
| `Esc` | zamknięcie |
| `/skill <nazwa>` | treść do kontekstu następnej tury + potwierdzenie w scrollbacku |
| `/skill` bez nazwy | otwiera `/skills` |
| `/skill nie-ma` | `nie znam skilla 'nie-ma' — wpisz /skills`, bez crasha |
| pasek statusu | `kroki N/max · narzedzia N · skille N · odmowy N · <stan>` |

`skill.load` wywołane przez model też ustawia znacznik „w kontekście" (modal
czyta `loaded_skills` z eventu `tool_call`). Lista jest czytana od nowa przy
każdym zadaniu i przy otwarciu modala — nowy plik na dysku działa bez restartu.

## 7. CLI

`ardntai skills` wypisuje tę samą listę z katalogiem źródłowym:

```
Skille (2):
  raport-dzienny       331 B    dzienny raport z postepu prac
                       zrodlo: workspace  plik: /.../skills/raport-dzienny/SKILL.md
                       kiedy: gdy trzeba podsumowac dzien
Lista w promptcie: 2 pozycji (95 znakow), tresc: /skill <nazwa> albo skill.load
```

## 8. Testy i dowody

| co | gdzie |
|---|---|
| dwa skille → oba odkryte, 2 pozycje w liście | `skills::tests::two_skills_in_workspace_are_discovered_and_listed` |
| workspace wygrywa z `~/.config` | `skills::tests::workspace_wins_over_config_for_the_same_name` |
| zepsuty YAML → start działa, oznaczony, nie wstrzyknięty | `skills::tests::broken_yaml_is_listed_but_not_injected`, `tui::tests::broken_header_skill_is_listed_but_not_loaded` |
| limit 20 pozycji i sortowanie | `skills::tests::list_is_capped_at_twenty_and_sorted` |
| 8 KB obcinane po granicy znaku | `tools::tests::skill_content_over_8kb_is_clipped_with_a_warning` |
| `skill.load` spoza listy → `NOT_FOUND` | `tools::tests::skill_load_outside_the_list_is_not_found` |
| lista i treść w żądaniu (atrapa providera) | `tests/cli.rs::skill_list_and_loaded_content_reach_the_request` |
| `/skill` w TUI → kontekst następnej tury | `tui::tests::skill_command_loads_content_into_the_next_turn` |
| modal, podgląd, znacznik z `skill.load` | `tui::tests::skills_modal_lists_name_description_and_size`, `enter_in_the_modal_previews_the_body_in_the_scrollback`, `skill_loaded_by_the_model_is_marked_in_the_modal` |

Zrzuty 960×608: `app/shots/r16-skills.png` (modal), `app/shots/r16-skill.png`
(scrollback po `/skill`, status `skille 2`). Oba robi
`--screenshot-skills` / `--screenshot-skill`.
