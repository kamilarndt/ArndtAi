# ADR-005 — Bramka jakości: hooki lokalne, hosted CI wyłączone

Data: 2026-11-20
Status: Accepted

## Kontekst

`.github/workflows/verify.yml` uruchamia `make verify` na GitHub Actions.
Przebieg `44693493790` nie wystartował: „recent account payments have failed or
your spending limit needs to be increased". Repozytorium jest prywatne.

## Decyzja

Bramką jest para hooków lokalnych (`pre-commit`, `pre-push`) i `make ci`.
Workflow zostaje w repo jako gotowy, ale nieaktywny.

## Konsekwencje

- Dowód zmiany żyje na maszynie autora do chwili pusha; po pushu — w historii gita.
- `checks/evidence.json` i `craft.json` są generowane lokalnie (poza historią),
  bo niosą znaczniki czasu; w historii zostają logi `checks/**/*.png` i dokumenty.
- Nowy klon musi raz wykonać `git config core.hooksPath scripts/hooks`.

## Warunek powrotu

Uregulowany billing GitHuba albo repo publiczne (darmowe minuty). Wtedy workflow
działa bez zmian; `make verify` jest jedynym kontraktem i nie trzeba go dublować.
