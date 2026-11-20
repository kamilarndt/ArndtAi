# HyAtlas-Memory — rozbiórka pod pamięć ArndtAi

Źródło: README repozytorium `tuancookiez-hub/HyAtlas-Memory` (v5.0.0, Go,
Apache-2.0), odczytane 2026-11-20. Spec `02-MEMORY.md` wymaga modelu warstw
i trybów, nie kodu — poniżej to, co da się przenieść bez zależności.

## Osiem warstw

| warstwa | co trzyma | w ArndtAi |
|---|---|---|
| L1 Profile | preferencje i styl użytkownika (mirror faktów) | CZĘŚĆ: fakty trafiają do `facts.md`, ale bez wydzielonego profilu |
| L2 Raw | każda pamięć jak przyszła | DONE: transkrypty JSONL + tabela `transcripts` + FTS |
| L3 Fact | wyekstrahowane atomy faktów | DONE: `facts.md` (ekstrakcja osobnym wywołaniem modelu) |
| L4 Summary | streszczenie per epizod pracy | DONE: `.agent-memory/episodes.md` (wiersz per przebieg) |
| L5 Knowledge | graf encji i relacji z dowodami | CZĘŚĆ: `graph.md` klastruje po temacie; brak encji i relacji |
| L6 Schema | powtarzalne wzorce uogólnione | BRAK (świadomie: wymaga modelu i masy danych) |
| L7 Intention | bieżący cel / następny krok | BRAK: intencja jest w `policies/`, nie w pamięci |
| L8 Entity | strona per projekt/narzędzie/osoba z historią | BRAK |

## Trzy tryby

| tryb | wywołania LLM | warstwy | konsolidacja |
|---|---|---|---|
| lite | brak | 1/8 (raw) | nie |
| pro | 1 na zapis + 1 na epizod | 6/8 (bez L5, L6) | nie |
| ultra | j.w. + wsad okresowy | 8/8 | tak, co 6 h |

ArndtAi: `lite` = transkrypty + FTS; `pro` = epizody + fakty (jedno wywołanie
po zadaniu); `ultra` = dodatkowo `consolidate` (graf tematyczny) — uruchamiane
ręcznie, nie w tle.

## Wnioski, które wdrożono

- **Rozdzielenie trybów po koszcie** — ekstrakcja tylko tam, gdzie jest sens
  (`ultra` domyślnie, `pro` za flagą) — patrz G22.
- **Konsolidacja jako osobny przebieg**, nie część pętli zadania — G27.
- **Prawo do cofnięcia**: HyAtlas nigdy nie kasuje, tylko oznacza. ArndtAi:
  `improve` tylko proponuje, a `gate_policy` wycofuje z snapshotu — G31.

## Czego świadomie nie ma

L5/L6/L8 w sensie HyAtlasa wymagają embeddingów i modelu przy każdym zapisie;
ArndtAi nie ma embeddera i nie chce nowej zależności. `graph.md` jest
deterministyczny i wystarcza do pytania „czego jest dużo", nie do „co jest
powiązane z czym". To zapisane ograniczenie, nie dług do spłaty w v1.
