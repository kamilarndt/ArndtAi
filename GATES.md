# Gates: ArndtAi — domknięcie M0/Fazy B zgodnie z audytem 2026-11-13

OWNS: app/src/**, app/Cargo.toml, docs/**, README.md, scripts/**, GATES.md

Scope: naprawa S1/S2 z `audyty/arndtai-2026-11-13.md` (F1-F8, F16, F20-F22, F12) i doprowadzenie projektu do `ready` w Craft Anything z realnymi dowodami.

- [x] G1: katalog `.agent` jest niedostępny przez symlink/alias (F1, F7)
  CHECK: cargo test --quiet tools::tests
  EXPECT: test result: ok.
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=bebc46b979e5d6380629e8d4f0934fff2260eda8f3f27b01d89e298860bcd821; exit=0; EXPECT=matched; output-sha256=532d3e37b41f0d8c7ccf4d1f42a18d3f183528b0bf0d9defb65eb5050d4b12fc; output-bytes=118; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G2: kroki są zapisywane i wznawiane bez powtórzenia efektu (F2, F8)
  CHECK: cargo test --quiet agent::tests
  EXPECT: test result: ok.
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=10e696243d702bde85398127268d9498cf3cfb76eb1cbacc3cc8d0fada10272e; exit=0; EXPECT=matched; output-sha256=14b9502d0f583f1ae4c9e323f196953cbe7e9deeb36828b5e0e82f370995415a; output-bytes=118; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G3: SQLite w trybie WAL z busy_timeout (F6)
  CHECK: cargo test --quiet store::tests::wal_and_busy_timeout
  EXPECT: test result: ok.
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=b529de5beaf4e4287691db2c5048b348d8f62ce5e0c17659a7c956e87ae07faf; exit=0; EXPECT=matched; output-sha256=e85c3ac850825ccf2b1f6815f1e020f8f9b0fdd2335af044c476e7192b9b4905; output-bytes=114; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G4: outbox jest transakcyjny i unikalny per krok (F4, F24)
  CHECK: cargo test --quiet store::tests::outbox_is_once_per_step
  EXPECT: test result: ok.
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=49ac8cca94e016a867dc540040322b5f03ea225b29dc2a3cf7483dce3bf7c04e; exit=0; EXPECT=matched; output-sha256=2c4b0b23315c3642ee70be9c726c4b503c95402068227b330b2462af258340b0; output-bytes=114; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G5: klucz API nie trafia do argv procesu curl (F5)
  CHECK: cargo test --quiet provider::tests
  EXPECT: test result: ok.
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=241bfe5486e928b01d03d7bb74f5ce4717089ace2aa18ad0b9b4bf753123c0cb; exit=0; EXPECT=matched; output-sha256=532d3e37b41f0d8c7ccf4d1f42a18d3f183528b0bf0d9defb65eb5050d4b12fc; output-bytes=118; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G6: powtórzony `step_id` w MCP nie wykonuje efektu drugi raz (F3)
  CHECK: cargo test --quiet mcp::tests
  EXPECT: test result: ok.
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=6cbaaf3490f3b648c052821344b3c75087eb3a354b8ca2f6cc32f8c7983acbd0; exit=0; EXPECT=matched; output-sha256=a2674e75de044a2d65e9c61974e72e2b8b1c1d371429e1f6521ceae31759391c; output-bytes=116; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G7: pełny zestaw testów projektu przechodzi
  CHECK: cargo test 2>&1 | tail -5
  EXPECT: test result: ok.
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=0e31f5f35aca98133ea5cc3c6fb57a38091557a75dbb0c9e2da4b4765838000c; exit=0; EXPECT=matched; output-sha256=14da33ef38e90db57a3513e3ef08c3c3a1b72d30a078c3d6390a93e9210a32a8; output-bytes=242; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G8: demo przechodzi 39/39 sprawdzeń
  CHECK: cargo run --quiet -- demo
  EXPECT: DEMO OK
  CWD: app
  EVIDENCE: automatic-evidence=v1; definition-sha256=409b2f4ab5160e1d865ca550d91bba940f3e85ab15b5c974d04c1d69a3efe3c4; exit=0; EXPECT=matched; output-sha256=ef5953af2470ae286228fd77ae054c7aba7953473e90b8158e313d7290176cfd; output-bytes=1558; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi/app; path=06cc7ff529f1/32 entries

- [x] G9: dowody są trwałe i spójne z `craft.json`, bez duplikatów `at_ms` (F12)
  CHECK: python3 scripts/verify.py --assert
  EXPECT: CHECKS OK
  CWD: .
  EVIDENCE: automatic-evidence=v1; definition-sha256=38b1d3dd34ab3c22afbfeb4965337d1e560357f56dbf0d29646bdd4b3beec2d8; exit=0; EXPECT=matched; output-sha256=86bbac4c12344c687d8f5fff3552aae5b4b152b2a34162ba180a2dbd52392533; output-bytes=10; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi; path=1a8468950027/40 entries

- [x] G10: deliverables Fazy B istnieją (F16, F28, F15)
  CHECK: test -f docs/contracts/events.md && test -f README.md && test -f docs/re/README.md && echo DELIVERABLES OK
  EXPECT: DELIVERABLES OK
  CWD: .
  EVIDENCE: automatic-evidence=v1; definition-sha256=2e78dcec970101bc31a8e9f776d68bfc0fc421cd93469e9d22c249ed29867126; exit=0; EXPECT=matched; output-sha256=cbf68df2f1f1c85cf96ed654bcdfac306cb070f779270610b4c9de9ec7dc9eee; output-bytes=16; shell=/bin/sh; cwd=/home/karndt/projects/ArndtAi; path=06cc7ff529f1/32 entries

- [x] G11: projekt w Craft Anything pokazuje `Ready` z realnymi checkami
  EVIDENCE: zrzut a11y (orca computer) — „ArndtAi, Ready", Study/Build/Check/Ready: Done, lista Check 1-5 z `cargo ... -> exit 0`; brak banera błędu po usunięciu starego `feed.json`

- [x] G12: TUI otwiera się i wykonuje zadanie (mock) — domknięcie etapu M0/Faza B (PLAN-DOKONCZENIA.md nazywa tę bramkę „G4")
  CHECK: cd app && cargo build --quiet && cargo run --quiet -- tui --provider mock --steps 2 -w /tmp/opencode/g4ws "write a.txt :: x"
  EXPECT: exit 0, brak panic
  CWD: app
  EVIDENCE: exit=0; build-exit=0; output-sha256=9d7e8140e8a5bc1397c68035e1a649987aff86a6c3c7c6bfc275bf67983a7302; screenshot=checks/tui-screenshot.png (960x608, 1751846 B, sha256=0eb2f25e59ff85d6e0dfc6f386160dcb71d5f739f9fab9b1f4acff57e7a6abe4); wzięte 2026-11-13 13:40 przez `cargo run -- --screenshot`; transkrypt sesji ma nazwę z timestampem, więc powtórka daje inny hash wyjścia

- [x] G13: testy na żywo (realny model + bramki) przechodzą w prawdziwym terminalu
  CHECK: cd app && cargo build --quiet && python3 -u ../scripts/live_test.py
  EXPECT: 8/8 scenariuszy bez zarzutu
  CWD: .
  EVIDENCE: 8/8 (2026-11-14 00:17) — TUI: zapis+odczyt, odmowa bramki zakresu (mock), odmowa na realnym modelu, brak pliku; CLI: wznowienie (mock i realny), budżet kroków, fs.append; harness sha256=efbaa82b1481c3647e619e64c17e5a94df8f7f430b0e51c1a4f0f4b25757b144; ekrany w /tmp/opencode/live/*.txt; wymaga pyte (pip install --target /tmp/opencode/pylibs pyte) i klucza w ~/.config/arndtai/ocgo.env
  NOTE: scenariusz 4b zależy od zachowania modelu (czy użyje fs.append); po rundzie 10 przechodzi

- [x] G14: parytet TUI z Grok Build w zakresie bez ich runtime'u (rundy 14–18)
  CHECK: cd app && cargo build --quiet && cargo test --quiet && cargo run --quiet -- demo
  EXPECT: testy zielone, DEMO OK
  CWD: app
  EVIDENCE: 231 unit + 15 integration, exit 0; demo 39/39; `docs/re/tui-parity.md` tabela stanu; zrzuty `app/shots/{r14-*,settings,ask,light,r16-skill,r17-*,markdown*}.png` (960×608 i 40 kolumn); `/export` na żywo → `eksport-20261114-0242.md` z nagłówkiem i krokami; `/copy` → `skopiowano 65 znakow (OSC 52)`; `skill.load` na żywo → `215 bajtow`, raport z datą 2026-11-14
  NOTE: nie obejmuje voice/share/plugin/workflow/dashboard/subagentów — wymagają runtime'u Groka
  NOTE: 2026-11-19 — po dodaniu testu backfillu FTS komenda bramki daje 232 unit (15 integration bez zmian); demo 39/39.

- [x] G15: utrata procesu w środku pracy (kill -9) nie powtarza efektu
  CHECK: cd app && cargo test --test cli killing_the_process
  EXPECT: 3 passed; 0 failed — `killing_the_process_while_the_model_thinks_does_not_repeat_the_step`, `killing_the_process_inside_the_tool_leaves_the_step_abandoned`, `mcp_resume_replays_a_done_step_without_reexecuting_it`
  CWD: app
  EVIDENCE: 2026-11-19, exit 0, 2/2 zielone. Krok `done` wraca z zapisanego wyniku (`tool_result_cached`, plik nietknięty), krok `proposed` bez wyniku jest `abandoned` (`tool_result_skipped`, `crash_before_result`). Mutanty: `same=false` → test 1 czerwony; `abandoned=false` → test 2 czerwony (proces wisi na FIFO, limit 30 s go ubija).
  NOTE: trzy testy; trzeci (`mcp_resume_replays_a_done_step_without_reexecuting_it`) idzie ścieżką domyślną (perimetr MCP) i pokazuje, że serwer oddaje wynik z `steps.result_json` — plik nie powstaje ponownie, mimo `ok=true` w wyniku. Wcześniejsza nota o „drugim wykonaniu przez MCP" była błędna: mieszano transporty (pierwszy bieg `--no-mcp`, wznowienie z MCP).

- [x] G16: Faza C — web UI czyta kontrakt eventów bez zmian w rdzeniu
  CHECK: cd app && cargo test --quiet && cargo run --quiet -- demo
  EXPECT: 232 unit + 20 integration, exit 0; demo 39/39
  CWD: app
  EVIDENCE: 2026-11-19 — `app/web/` (HTML+JS, zero zależności); w Orca browser na `http://127.0.0.1:8765/app/web/`: 12/12 typów `kind`, konsola bez wpisów; dopisanie pełnej linii → +1 wiersz w 2 s bez przeładowania; JSON bez `\n` → 0 nowych wierszy i 0 błędów, po dopisaniu `\n` → wiersz z treścią; `sessionUrl('../../etc/passwd')` → „zła nazwa sesji”; filtr `kind` ukrywa i pokazuje wiersze. Zrzut `app/shots/faza-c-sesja.png`; `checks/round-18/` z CA.
  NOTE: MVP czyta wyłącznie JSONL; `state.db` i sterowanie zadaniami są poza zakresem. Render markdown w web UI — kolejny krok.

- [x] G17: v0.1 — czysta instalacja + realne zadanie na żywym modelu
  CHECK: cargo install --path app --root /tmp/ardntai-install && /tmp/ardntai-install/bin/ardntai --version
  EXPECT: binarka powstaje, `ardntai 0.1.0`, exit 0
  CWD: repo
  EVIDENCE: 2026-11-19 — `cargo install` → 4,25 MB binarka; `--version` → `ardntai 0.1.0` (flaga dodana tego dnia; wcześniej była traktowana jak treść zadania). Czysty workspace `/tmp/live-v01b`, realny model `mimo-v2.6-flash` przez OC-Go: `Status: DONE, kroki 3, narzędzia 2, odmowy 0, dryf 0`, `one.txt` = `alfa` (4 B), transkrypt `app/shots/live-v01b-sesja.jsonl` (task → 2× tool_call/tool_result → summary). Perimetr `mcp`.
  NOTE: nie uruchamiałem `scripts/live_test.py` (wymaga `pyte`, brak w systemie) — dowód TUI na żywo pochodzi z 14 listopada; ten przebieg to CLI na release.

- [x] G18: kontrakt eventów v2 — przejścia bramek w transkrypcie
  CHECK: cd app && cargo test --quiet && cargo run --quiet -- demo
  EXPECT: 242 unit + 22 integration, exit 0; demo 39/39
  CWD: app
  EVIDENCE: 2026-11-19 — `app/src/agent.rs` pisze `kind: "gate"` z `stage` i `detail` na 8 etapach (RECEIVED, PLANNING, STEP_PROPOSED, SCOPE_CHECK, POLICY_CHECK, EXECUTING, OBSERVE, STOP_CHECK); test `tests/cli.rs::gate_stages_are_written_to_the_transcript` sprawdza obecność i kolejność. TUI ignoruje `gate` (telemetria, testy TUI zielone), web UI rysuje wiersz GATE (sprawdzone w Orca browser: 1 wiersz, 13/13 typów, konsola czysta). Fixture: `python3 app/web/fixture_events.py --out /tmp/fx-gate` → 13 linii.
  NOTE: `docs/contracts/events.md` podniesiony do v2; konsument v1 czyta dalej (nieznany `kind` pomija).
