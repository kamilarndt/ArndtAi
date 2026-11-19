---
name: przyklad
description: Wzór skilla projektu — pokazuje format, nie robi nic sam.
when: gdy chcesz zobaczyć, jak wygląda poprawny SKILL.md w tym repo
---

# Skill: przyklad

To jest wzór. Skopiuj katalog, zmień `name`, `description` i treść.

## Kiedy używać

Gdy zadanie wymaga powtarzalnej procedury, którą można opisać krokami
(np. „dodaj nowy tool do perimetru”).

## Kroki

1. Sprawdź, czy narzędzie już istnieje: `ardntai tools`.
2. Dopisz implementację w `app/src/tools.rs` i schemat w `input_schema`.
3. Dodaj test w `app/src/tools.rs` (happy path + ścieżka poza workspace).
4. Uruchom `make verify` — commit bez zielonych dowodów nie przejdzie.

## Czego nie robić

Nie dodawaj zależności bez zgody; nie zmieniaj zamrożonych kontraktów
bez podniesienia ich wersji.
