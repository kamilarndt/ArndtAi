# Instrukcja dla agenta AI: dokończenie ArndtAi

**Cel:** Dokończyć M0, dodać recovery + single-writer, zbudować frontend w stylu DeepSeek (web) oraz utrzymać CLI.  
**Zasada:** Najpierw reverse-engineering wskazanych repozytoriów, potem implementacja. Nie kopiuj kodu 1:1 — bierz wzorce.

---

## 0. Kandydaci do reverse-engineering (REA)

Rozbierz poniższe repozytoria **przed** implementacją frontendu i komunikacji. Zapisz wnioski (własnymi słowami + lokalizatory) w `docs/re/`.

| Cel | Repozytorium | Co wziąć |
|-----|--------------|----------|
| Web UI + trajectory + sesje | https://github.com/deepseek-ai/deepseek-harness | Web UI (`npx @deepseek-ai/dsh web`), trajectory view, session ledger, plugin architecture |
| Proaktywność + single-writer + outbox | https://github.com/deepseek-ai/dsh-superagent-prototype | Trigger handling, ticket/work item, session-per-task, default-deny, attention budget |
| CLI + TUI + headless | https://github.com/xai-org/grok-build | TUI, headless (`grok -p`), MCP, sessions, skills |
| Alternatywny CLI (jeśli potrzebny) | https://github.com/superagent-ai/grok-cli | Terminal agent patterns, streaming, sub-agents |

**Polecenie dla REA:**  
„Rozbierz powyższe repozytoria. Dla każdego: jak wygląda pętla, jak komunikuje się z UI (web/CLI), jak trzyma sesje i eventy, jak robi proaktywność. Zrób dwie opcje komunikacji dla ArndtAi: **web** (chat + trajectory + stats) i **CLI** (istniejący terminal + ewentualny TUI). Zapisz decyzje „wzięte / odrzucone”.”

---

## 1. Stan wyjściowy (co już jest)

- Rdzeń Rust: pętla, bramki (allowlist, scope, drift), MCP stdio, sandbox `fs.*`, outbox plikowy, pamięć Lite/Pro (pliki), eventy sesji w `.agent/sessions/`.
- Brak: SQLite single-writer, durable recovery, walidacji MCP schema, frontendu web.

---

## 2. Kroki do wykonania (kolejność obowiązkowa)

### Faza A — Reverse-engineering (przed kodem)
1. Sklonuj lub przejrzyj repozytoria z sekcji 0.
2. Zapisz w `docs/re/` krótkie notatki: co wziąć do web UI, co do CLI, co do proaktywności.
3. Nie implementuj nic nowego, dopóki notatki nie są gotowe.

### Faza B — Domknięcie M0 (Critical + High z audytu)
1. **SQLite single-writer**  
   - Dodaj SQLite (WAL). Tabele: `tasks`, `sessions`, `transcripts`, `outbox`.  
   - Tylko rdzeń pisze. Perimeter nie ma DB.
2. **Durable state + recovery**  
   - Przed wykonaniem toola zapisz proponowany krok + decyzję bramki.  
   - Po wykonaniu zapisz wynik z `step_id`.  
   - Przy restarcie nie powtarzaj już wykonanego `step_id`.
3. **Idempotencja tool calls**  
   - Sprawdź w DB czy `step_id` już istnieje zanim wywołasz narzędzie.
4. **Transakcyjny outbox**  
   - Rekord outbox w tej samej transakcji co zmiana stanu zadania.
5. **Walidacja MCP**  
   - Przed `tools::call` waliduj argumenty względem JSON Schema z `tools/list`.
6. **Sandbox — drugie sprawdzenie**  
   - Po `canonicalize` jeszcze raz sprawdź `starts_with(root)` tuż przed otwarciem pliku.
7. **Schemat eventów**  
   - Zamroź format w `docs/contracts/events.md` (type, step, ts, payload).  
   - UI będzie go konsumować.

**Akceptacja Fazy B:**  
- `cargo test` przechodzi.  
- Demo + nowy test recovery (kill po pierwszym toolu, wznowienie bez podwójnego wykonania).  
- Path traversal i symlink nadal odrzucane.

### Faza C — Frontend w stylu DeepSeek (web)
1. Na podstawie reverse-engineering DeepSeek Harness zbuduj web UI.
2. **Wymagane elementy:**
   - Chat (historia + streaming).
   - Trajectory / ledger kroków (User → Assistant → Tool → Gate → Stop) z czasem i statusem.
   - Inspector (kliknięcie → input/output/błąd/argumenty).
   - Stats (tokeny, liczba tool calls, czas, bramki).
3. **Źródło danych:** eventy z sesji (schemat z Fazy B) + SSE lub polling.
4. **Backend:** istniejący `arndtai` + nowy endpoint HTTP (axum lub podobny) do listy sesji, eventów i `POST /run` ze streamem.
5. **Styl:** zbliżony do DeepSeek web (czysty, boczny panel trajectory/stats).

**Akceptacja Fazy C:**  
- Można rozmawiać z agentem przez przeglądarkę.  
- Widać kroki, tool calls i bramki w czasie rzeczywistym.  
- Inspector pokazuje szczegóły klikniętego kroku.

### Faza D — CLI (utrzymanie + ewentualne rozszerzenie)
1. Istniejący CLI (`arndtai run`, `demo`, `mcp`) musi nadal działać.
2. Opcjonalnie: prosty TUI inspirowany Grok CLI (nie obowiązkowe na start).
3. Obie opcje (web i CLI) korzystają z tego samego rdzenia i tego samego schematu eventów.

### Faza E — Następne funkcje (po akceptacji C+D)
1. Proaktywność (trigger inbox + shadow mode + default-deny) — wzorce z dsh-superagent.
2. Pamięć Pro (fakty + epizody).
3. Skills loader z walidacją frontmatter.
4. Self-improvement tylko deklaratywne + eval gate.

---

## 3. Zasady dla agenta wykonującego

- Przed każdym większym mechanizmem: reverse-engineering (sekcja 0) + notatka „wzięte/odrzucone”.
- Nie kopiuj kodu z repozytoriów. Bierz wzorce.
- Single-writer: tylko rdzeń pisze do SQLite.
- Perimeter (MCP tools) zostaje w Rust. Rdzeń może zostać w Rust na razie (migracja do Pythona później, jeśli zdecydujemy).
- Każda faza kończy się testami i krótkim raportem co zostało zrobione.
- Frontend web nie może omijać bramek ani sandboxu — wszystko idzie przez rdzeń.

---

## 4. Deliverables

- `docs/re/` — notatki z reverse-engineering.
- `docs/contracts/events.md` — schemat eventów.
- Działający SQLite + recovery.
- Web UI (chat + trajectory + inspector + stats).
- CLI nadal działa.
- Krótki README jak uruchomić web i CLI.

---

**Start:** Zacznij od Fazy A (REA na repozytoriach z sekcji 0). Dopiero potem Faza B.
