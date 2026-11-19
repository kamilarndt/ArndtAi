# docs/re — notatki decyzyjne (pause point Fazy A)

Ten katalog miał powstać **przed** implementacją Fazy B. Powstał po niej —
to jest dług zapisany w audycie (`F15`), a nie porządek. Poniżej stan
faktyczny z 2026-11-13, spisany po naprawach, z jawnym oznaczeniem, co było
odstępstwem.

## Co zostało ustalone

1. **Rdzeń w Rust, nie Python.** `09:97` dopuszcza „na razie"; wybór nie ma
   ADR-a porównującego warianty (`F17`). Uzasadnienie praktyczne: pętla
   i perimetr dzielą typy `Value`/`ToolResult`, brak drugiego procesu do
   utrzymania. Migracja do Pythona pozostaje otwarta — dlatego kontrakty
   (`docs/contracts/`) są językowo neutralne.
2. **Bramki: dwie w M0** (allowlista, budżet), zgodnie z ADR-002, nie cztery
   z `05:9-13`. Detektor dryfu **jest bramką** w kodzie (`agent.rs`: odcisk
   `tool+args` powtórzony w oknie 4 wywołań, w tym A,B,A,B), a nie tylko
   licznikiem — to świadome rozszerzenie po audycie (`F11`); ADR-002 opisuje
   zakres M0, nie stan po naprawach (`F18`).
3. **Idempotencja per `(session_id, step_id)`.** `step_id` jest kluczem
   kontraktu MCP; ten sam identyfikator z innymi argumentami jest błędem
   klienta i jest odrzucany, nie wykonywany ponownie.
4. **At-most-once po crashu.** Krok `proposed` bez wyniku jest zamykany jako
   `abandoned` i **nie** jest powtarzany. Świadomy wybór: powtórzenie efektu
   ubocznego (nadpisanie pliku, wysłanie powiadomienia) jest gorsze niż jego
   brak, a brak jest widoczny w transkrypcie (`tool_result_skipped`).
5. **Sandbox porównuje cel, nie nazwę.** Zakaz `.agent` działa na
   scanonicalizowanej ścieżce, więc alias/symlink do `.agent` jest odrzucany
   (reprodukcja z audytu `F1` przestaje przechodzić).
6. **Sekret nie idzie w `argv`.** Nagłówki HTTP trafiają do pliku `0600`
   podawanego curlowi przez `--config`; w `ps`/`/proc` widać tylko nazwę pliku
   (zamyka `F5`).

## Czego świadomie nie ma

- Frontend (Faza C) — brak w tym kroku; kontrakt eventów jest gotowy.
- Tabela `tasks` i tabela `transcripts` — patrz `docs/contracts/events.md`,
  sekcja „Odstępstwa".
- Testy zabijające proces (kill -9 w środku toola). Zamiast tego stan
  `proposed` jest odtwarzany wprost w teście (`agent::tests`), bo to on
  definiuje zachowanie po crashu; zabijanie procesu testowałoby OS, nie kod.

## Notatki z badań

- `grok-build-tui.md` — wzorzec ekranu TUI wzięty z pagera Grok Build
  (`xai-grok-pager`, kopia w `reference/grok-build`): co przyjęto, co odrzucono
  i czego nie udało się zmierzyć. Opis gotowego trybu: `docs/TUI.md`.

## Gdzie szukać dowodów

- `GATES.md` — bramki i komendy, które je rozstrzygają.
- `scripts/verify.py` — odtwarza `checks/` i wpisy `craft.json` (exit code, hash
  wyjścia, hash drzewa).
- `audyty/arndtai-2026-11-13.md` — audyt wejściowy i jego znaleziska
  (F1-F32), do których ten katalog się odnosi.
