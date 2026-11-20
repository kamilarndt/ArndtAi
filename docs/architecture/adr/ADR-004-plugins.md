# ADR-004 — Pluginy in-process: poza zakresem v1

Data: 2026-11-20
Status: Accepted

## Kontekst

Spec `04-SKILLS-AND-PLUGINS.md` przewiduje pluginy działające in-process obok
MCP, z własnymi bramkami i formatami pamięci. Audyt
(`docs/audits/spec-gap-2026-11-19.md`, D04) wskazał brak API, rejestru i katalogu.

## Decyzja

v1 nie ma pluginów. Perimetr to MCP plus wbudowane narzędzia
(`fs.read/write/append`, `notify.send`, `skill.load`, `memory.ingest`).

## Uzasadnienie

- Plugin to drugi format uprawnień: musi przejść te same bramki (allowlista,
  zakres, dryf) co perimetr. Zrobiony naprędce byłby dziurą, nie rozszerzeniem.
- Skille (`SKILL.md`) pokrywają dziś potrzebę „procedury bez nowego kodu".
- Każdy plugin to nowa powierzchnia utrzymania i testów; v1 ma być przewidywalne.

## Warunek powrotu

Wraca, gdy pojawi się potrzeba narzędzi w innym języku niż Rust/MCP albo gdy
skille przestaną wystarczać (np. plugin musi trzymać stan między krokami).
Wtedy: ADR z API, rejestrem, miejscem w bramkach i testem, że plugin nie omija
zakresu workspace.
