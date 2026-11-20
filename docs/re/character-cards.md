# Karty postaci (SillyTavern / Character Card V2) — rozbiórka

Źródło: `malfoyslastname/character-card-spec-v2`, `spec_v2.md`, odczytane
2026-11-20. Spec `03-PERSONALITY.md` wymaga własnego formatu opartego na polach
karty — poniżej minimalny zestaw, który ma sens dla agenta wykonawczego.

## Pola V1 (każda karta je ma)

```
name, description, personality, scenario, first_mes, mes_example
```

## Co dodaje V2

`spec: "chara_card_v2"`, `spec_version: "2.0"` oraz w `data`:
`creator_notes`, `system_prompt`, `post_history_instructions`,
`alternate_greetings`, `character_book` (lorebook), `tags`, `creator`,
`character_version`, `extensions`.

`character_book` to lista wpisów: `keys`, `content`, `enabled`,
`insertion_order`, `case_sensitive`, `selective` + `secondary_keys`,
`constant`, `position`. Wpisy są wstrzykiwane po słowach kluczowych.

## Co z tego bierze ArndtAi

| pole karty | odpowiednik | stan |
|---|---|---|
| `name`, `description` | nagłówek `personality/base.md` | DONE (cały plik jako blok) |
| `system_prompt` | instrukcje w `base.md` | DONE |
| `character_book` (lorebook) | brak — kontekst bierzemy z `facts.md` i `episodes.md` | ŚWIADOMIE POZA ZAKRESEM |
| `tags`, `creator`, `character_version` | brak | nieistotne dla agenta |
| `alternate_greetings`, `first_mes` | brak | to cecha czatu z postacią, nie agenta wykonawczego |

## Decyzja

Nie implementujemy parsera kart: agent nie prowadzi rozmowy wcieleniowej,
tylko wykonuje zadania. `personality/base.md` jest jednym blokiem tekstu —
prostszy, bez ryzyka, że nieznane pola znikną przy zapisie (wymóg V2:
„MUST NOT destroy unknown key-value pairs").

Warunek powrotu: gdy pojawi się tryb rozmowy z postacią albo import kart
od użytkowników — wtedy parser pól + `character_book` jako źródło kontekstu.
