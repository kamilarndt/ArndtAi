# Brief dla Groka — ArndtAi, 2026-11-19

Wklej poniżej linię „---” do czatu. Wszystko zmierzone dziś w repo, nie z pamięci.

---

## Co to jest

ArndtAi to samodzielny agent wykonawczy w Rust: dostaje zadanie, planuje kroki,
woła narzędzia przez perimetr MCP (`fs.read`, `fs.write`, `fs.append`,
`notify.send`, `skill.load`), każdy krok zapisuje w SQLite (WAL), a transkrypty
eksportuje do JSONL. Bramki decydują, czy zapis przechodzi bez pytania.
Interfejs: TUI w ratatui + tryb CLI. Rozwijany rundami przez Craft Anything,
wzorzec wizualny: pager `xai-org/grok-build`.

Struktura: `app/` (Rust, ~21,5 tys. linii, 20 modułów), `docs/` (kontrakty,
ADR-001..004, STATUS, TUI), `scripts/verify.py` (dowody), `GATES.md` (G1–G14).

## Dowód stanu (uruchomione 2026-11-19)

- `cargo test` → 232 unit + 15 integration, exit 0
- `cargo run -- demo` → 39/39, DEMO OK
- `python3 scripts/verify.py --assert` → CHECKS OK
- Zamknięte: M0 + Faza B, parzystość TUI w zakresie bez runtime'u Groka
  (rundy 14–18: `/model`, `/settings`, `/skills`, `/copy`, `/export`, markdown,
  okno kontekstu z metadanych modelu), testy na żywo 8/8 (realny model OC-Go,
  pty + pyte), FTS5 z backfillem starej bazy (test dodany 19 listopada).
- Ostatnia zmiana produkcyjna: 14 listopada 03:09. Od tego czasu tylko test
  backfillu i dokumenty.

## Co zostaje (M1, z `docs/STATUS.md`)

1. Faza C — web UI na gotowym kontrakcie eventów (`docs/contracts/events.md`).
2. Testy `kill -9` — odporność na utratę procesu w środku narzędzia
   (semantyka at-most-once jest, pokrycia brak).
3. Python core — ADR-004 status **Proposed**; dziś cały rdzeń w Rust.
4. Historia git — **brak repozytorium**; `app/` waży 984 MB (głównie `target/`).
5. Streaming tokenów — blok rośnie w miejscu, ale pętla dostaje jedną myśl na
   krok, więc efektu nie widać do zmiany providera.

## Znane słabe punkty

- Brak git = brak historii, brak `git bisect`, brak review diffa; 5 dni pracy
  stoi bez kopii poza dyskiem.
- `scripts/live_test.py` nie był uruchamiany od 14 listopada; ekrany w `/tmp`
  wyczyszczone, więc dowód G13 jest tylko wpisem w `GATES.md`.
- `verify.py --assert` jest czerwony po każdej zmianie w `src/` aż do
  ponownego `python3 scripts/verify.py` — łatwo zostawić G9 w kłamstwie.
- TUI ma 280 KB w jednym pliku (`app/src/tui.rs`), `agent.rs` i `provider.rs`
  po ~45–50 KB.

## Pytania do Groka (w tej kolejności)

1. Co pierwsze: repozytorium git z historią (ryzyko utraty) czy testy `kill -9`
   (ryzyko fałszywego „gotowe")? Moja rekomendacja: git, bo bez niego każda
   dalsza runda jest nieodwracalna.
2. Czy TUI ma zostać w Rust i czy w ogóle iść w Fazę C (web UI) przed
   ustabilizowaniem rdzenia? Kontrakt eventów jest gotowy, ale nie ma jeszcze
   drugiego klienta, który by go sprawdził.
3. ADR-004: zostawić rdzeń w Rust czy migrować do Python core + Rust perimeter?
   Decyzja odroczona na M1; koszt rośnie z każdą rundą.
4. Czy parzystość z `grok-build` jest jeszcze celem, czy wystarczy funkcjonalnie
   równoważny TUI? Rundy 14–18 kosztowały najwięcej czasu w projekcie.

## Czego Grok nie może sprawdzić

Nie ma dostępu do repo. Wszystkie liczby powyżej pochodzą z dzisiejszego
uruchomienia; jeśli chce pliki, trzeba wkleić zawartość.
