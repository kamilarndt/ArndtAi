# `--resume` psuje numerację kroków i zwraca stary wynik (runda 10)

Data: 2026-11-14. Zakres: `app/src/agent.rs`, `app/src/store.rs`, testy w
`agent.rs`. Kontrakty (`docs/contracts/*`), bramki, allowlista, detektor dryfu,
`notify.send`, outbox, TUI i web UI bez zmian.

## 1. Co było zepsute

**Było:** każdy przebieg liczył kroki od 1. Po `--resume` (ta sama sesja) nowy
krok dostawał `step_id = 1`, a pod tym numerem w SQLite leżał już krok
poprzedniej tury:

- krok `done` → zwracany z `result_json` jako `tool_result_cached`: świeży
  `fs.read` pokazywał zapamiętany `NOT_FOUND`, choć plik już istniał;
- ten sam `step_id` z innymi argumentami → w perimetrze MCP `DENIED`
  („step_id 2 jest juz uzyty z innymi argumentami”), a w trybie wprocesowym
  ciche wykonanie pod starym numerem i nadpisanie wyniku historii;
- po trzech kolizjach z rzędu sesja kończyła się `FAILED_STALLED`.

Objaw z transkryptu 1794614370: tura 1 = kroki 1–3, po `--resume` krok 1 wracał
z cache, kroki 2 i 3 dostawały `DENIED`.

## 2. Jak naprawione

**Granica tury w SQLite.** Nowa tabela `turns(session_id, turn_no, status,
base_step, started_at, finished_at)` — wewnętrzny stan magazynu, nie kontrakt
eventów. `Store::open_turn` przy starcie przebiegu:

- ostatnia tura `running` (proces zginął) → **wznawiamy ją** z jej `base_step`;
- tura zamknięta albo sesja sprzed tej rundy → **nowa tura** z
  `base_step = max(step_id w sesji) + 1`.

**Numeracja.** `step_id` (i pole `step` w JSONL) = `base_step + (krok w turze − 1)`.
Pierwszy krok nowej tury to zawsze `max + 1`, więc identyfikatory z historii
nigdy się nie powtarzają. `steps` w `Report`/`summary` dalej liczy tylko bieżącą
turę i pilnuje budżetu `--steps`.

**Zakres idempotencji.** Odczyt zapisanego kroku (`tool_result_cached`, skip
`abandoned`) i reguła „ten sam `step_id` z innymi argumentami = `DENIED`”
działają wyłącznie dla numerów od `base_step` w górę, czyli w bieżącej turze.
Kroki z poprzednich tur są historią: nie blokują nowego wywołania i nie oddają
swojego wyniku.

**At-most-once bez osłabienia.** Tura przerwana crashem wznawia się od swojego
`base_step`: kroki `done` wracają z zapisanego wyniku (zero efektu), a krok
`proposed` bez wyniku → `abandoned` + `tool_result_skipped`, bez powtórzenia
efektu. Po normalnym zakończeniu (`finish`) tura jest zamykana
(`Store::close_turn`), więc następne wznowienie zaczyna nową turę.

## 3. Czego nie ruszono

- Bramki (allowlista → scope guard → dryf), próg `FAILED_STALLED`, `notify.send`
  i outbox — ten sam kod i te same testy.
- Format transkryptu i `docs/contracts/events.md`: żadnego nowego typu eventu,
  `step` to nadal liczba całkowita (tylko liczona dalej niż 1).
- TUI: ekran, klawisze i układ bez zmian — TUI dalej woła `agent::run`, a każda
  kolejna komenda w tej samej sesji jest teraz nową turą.
- Web UI nie powstało, Python core nie doszło.

## 4. Dowody

Czerwone przed poprawką (same testy, stary kod):

- `new_turn_continues_the_session_numbering` — kroki nowej tury `[1,2,3]`
  zamiast `[1,2,3,4]`.
- `old_result_is_not_replayed_in_a_new_turn` — ostatni `fs.read` zwrócił
  zapamiętane `NOT_FOUND` (`ok=false`) po tym, jak plik powstał.

Zielone po poprawce:

- `cargo build` — exit 0, bez ostrzeżeń.
- `cargo test` — 104 testy jednostkowe + 4 integracyjne, wszystkie zielone.
  Nowe: `new_turn_continues_the_session_numbering`,
  `old_result_is_not_replayed_in_a_new_turn`,
  `new_turn_can_reuse_a_call_with_other_arguments`,
  `resume_after_append_keeps_both_turns`, `one_turn_never_reuses_a_step_id`,
  `turn_boundary_follows_history_and_resumes_a_crash` (magazyn),
  `crashed_turn_replays_its_done_step_without_repeating_effect` (zastępuje
  `resume_skips_done_step_...`, które utrwalało stary, błędny replay między
  turami), `resume_marks_proposed_step_abandoned_without_executing` (seeduje
  turę `running`, czyli prawdziwy crash).
  Ten sam `step_id` z innymi argumentami w jednej turze nadal daje `DENIED`:
  `mcp::tests::repeated_step_id_does_not_repeat_side_effect` bez zmian.
- `cargo run -- demo` — 39/39, `DEMO OK`.
- Żywy przebieg CLI (perimetr MCP, `/tmp/r10ws`): tura 1 = kroki 1–3, tura 2
  `--resume` = kroki 4–6, plik `beta`, `Odmowy: 0`, `Dryf: 0`, `DONE`.
- `cargo run -- --screenshot shots/now.png` — PNG 960×608, ekran sesji bez zmian
  układu (kroki, pasek statusu, prompt).
