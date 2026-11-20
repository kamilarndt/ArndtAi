# AGENTS.md — ArndtAi

Ten plik mówi agentowi (i człowiekowi), **czym jest projekt, gdzie leży kod i jak
wolno go zmieniać**. Dokumenty opisowe: `README.md` (start), `docs/STATUS.md`
(stan), `docs/ARCHITECTURE.md` (budowa), `GATES.md` (bramki akceptacji).

## Czym jest projekt

ArndtAi to samodzielny agent wykonawczy w Rust: pętla decyzyjna z bramkami,
perimetr narzędzi MCP (`fs.read`, `fs.write`, `notify.send`), trwały stan
w SQLite (WAL) i transkrypty JSONL. Nie jest pluginem Hermesa ani klonem
innego agenta. Szczegóły: `docs/ARCHITECTURE.md`.

## Układ katalogów

```
app/                  # JEDYNE miejsce z kodem produkcyjnym
  src/*.rs            # 25 modułów, m.in. tui.rs, proactive.rs, eval.rs
  tests/cli.rs        # testy integracyjne CLI
  Cargo.toml, Cargo.lock
docs/                 # dokumenty zamrożone i opisowe
  contracts/          # mcp.md, events.md — formaty, zmiana = nowa wersja
  architecture/adr/   # ADR-001..003 — decyzje
  re/                 # notatki decyzyjne, odstępstwa, dług
  ARCHITECTURE.md, STATUS.md, README.md, TUI.md
scripts/verify.py     # dowody do checks (exit code, hash wyjścia, hash drzewa)
craft-anything-input/ # materiał wejściowy (spec 00-09) — NIE kod
audyty/               # raport audytu 2026-11-13 + paczka zip
GATES.md              # bramki G1-G11 z dowodami
```

Kod jest w `app/`, bo projekt prowadzi Craft Anything w trybie „start-from":
korzeń trzyma materiał i dokumenty, aplikacja żyje w `app/`.

## Komendy

```sh
cd app
cargo build                 # debug
cargo test                  # 264 unit + 29 integration
cargo run -- demo           # 47 sprawdzeń na realnych artefaktach
cargo run -- tui            # tryb terminalowy (docs/TUI.md)
cargo run -- -w /tmp/ws "write a.txt :: tresc; read a.txt"
cargo run -- -w /tmp/ws --resume "dopisz do a.txt druga linie"   # --resume wymaga tekstu zadania
cargo run -- --screenshot out.png   # zrzut do PNG, tryb bezobsługowy
python3 ../scripts/verify.py        # dowody (wymaga craft.json w korzeniu)
```

Z korzenia repo:

```sh
make verify                 # build, testy, demo + spojnosc dowodow (checks/, craft.json)
make test                   # tylko cargo test
git config core.hooksPath scripts/hooks   # raz na klon: hook pre-commit = make verify
```

Hook `pre-commit` odrzuca commit, gdy `make verify` padnie (a `pre-push`
sprawdza to samo przed wypchnięciem). Dowody `checks/*.log`,
`checks/evidence.json` i `craft.json` są **generowane przy każdym przebiegu**
i nie idą do historii — powód: zawierają czasy, które zmieniają się bez zmiany
kodu. W repo zostają zrzuty (`app/shots/`, `checks/**/*.png`) i dokumenty.
Uczciwość bramki: „dowód przed twierdzeniem” dotyczy istnienia sprawdzalnego
wyniku, nie commitowania zmiennych znaczników czasu.

## Zasady pracy

1. **Nie zmieniaj formatów zamrożonych bez nowej wersji.** `docs/contracts/mcp.md`
   i `docs/contracts/events.md` są konsumowane przez przyszły frontend (Faza C).
2. **Jedno źródło prawdy o stanie:** SQLite `.agent/state.db`. Plik
   `notifications.log` jest eksportem, nie źródłem.
3. **Idempotencja per `(session_id, step_id)`.** Ten sam `step_id` z innymi
   argumentami to błąd klienta, nie powtórzenie.
4. **At-most-once po crashu:** krok `proposed` bez wyniku → `abandoned`, bez
   powtórzenia efektu. Nie „naprawiaj" tego przez retry.
5. **Sekret nigdy w `argv`.** Nagłówki idą do pliku `0600` przez `curl --config`.
6. **Sprzątanie: `mv` do `~/archive/`, nie `rm -rf`.** Cache builda usuwaj
   `cargo clean`. Nie usuwaj `audyty/` ani `craft-anything-input/`.
7. **Dowód przed twierdzeniem.** Zmiana jest gotowa, gdy `cargo test` przechodzi,
   a dla bramek z `GATES.md` istnieje wpis z exit code i hashem.
8. **Nie otwieraj `.env`, kluczy ani `~/.ssh`.** Do testów providera używaj
   `--provider mock`.

## Gdzie szukać

| pytanie | plik |
|---|---|
| co to jest i jak uruchomić | `README.md` |
| jak zbudowane (moduły, pętla, magazyn) | `docs/ARCHITECTURE.md` |
| co jest zrobione, co zostało | `docs/STATUS.md` |
| jakie bramki i czym je sprawdzić | `GATES.md` |
| formaty MCP i eventów | `docs/contracts/` |
| dlaczego tak, a nie inaczej | `docs/architecture/adr/`, `docs/re/README.md` |
| tryb terminalowy (ekran, klawisze) | `docs/TUI.md` |
| co było zepsute w audycie | `audyty/arndtai-2026-11-13.md` |
| wymagania wejściowe | `craft-anything-input/00-09*.md` |
