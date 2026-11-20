# ADR-005 — Bramka jakości: hooki lokalne + hosted CI

Data: 2026-11-20
Status: Accepted (znowelizowany tego samego dnia)

## Kontekst

`.github/workflows/verify.yml` uruchamia `make verify` na GitHub Actions.
Przebieg `44693493790` nie wystartował: „recent account payments have failed or
your spending limit needs to be increased". Repozytorium jest prywatne.

## Decyzja (pierwotna)

Bramką jest para hooków lokalnych (`pre-commit`, `pre-push`) i `make ci`.
Workflow zostaje w repo jako gotowy, ale nieaktywny.

## Nowelizacja 2026-11-20 (repo publiczne)

Repozytorium zostało upublicznione (`visibility: public`), więc minuty Actions
są darmowe. Workflow `verify.yml` jest **aktywny**: przebieg
[44702648228](https://github.com/kamilarndt/ArndtAi/actions/runs/44702648228)
przeszedł w całości (checkout, Rust, cache, `make verify`, artefakt dowodów).
Hooki lokalne zostają — łapią błąd przed pushem i nie kosztują kolejki.

## Konsekwencje

- Dowód zmiany: lokalnie hooki, po pushu — przebieg Actions widoczny publicznie.
- `checks/evidence.json` i `craft.json` są generowane lokalnie (poza historią),
  bo niosą znaczniki czasu; w historii zostają logi `checks/**/*.png` i dokumenty.
- Nowy klon musi raz wykonać `git config core.hooksPath scripts/hooks`.

## Warunek powrotu

Uregulowany billing GitHuba albo repo publiczne (darmowe minuty). Wtedy workflow
działa bez zmian; `make verify` jest jedynym kontraktem i nie trzeba go dublować.
