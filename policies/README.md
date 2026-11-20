# Polityki proaktywne

Reguły sprawdzane przez `ardntai tick` (bez modelu, bez sieci). Format JSON
(spec 06 mówi `*.yaml`; YAML wymagałby zależności — te same pola).

| pole | znaczenie |
|---|---|
| `name` | nazwa reguły (trafia do notatki i logu) |
| `when` | `session_failed`, `denied`, `idle_hours` |
| `threshold` | dla `denied`: minimalna liczba; dla `idle_hours`: godziny |
| `cooldown_hours` | ten sam wyzwalacz nie powtórzy się w tym oknie |
| `shadow_days` | >0: przez tyle dni reguła tylko loguje (shadow), potem działa |
| `note` | tekst notatki; wykonanie dopisuje status, odmowy i ciszę |

Efekt: wiersz w `.agent-memory/notes.md` i wpis w `.agent/run` → log
`.agent/policies.log`. Brak pasującej reguły = wpis „brak reguly — porzucone”
(default-deny z `06-PROACTIVITY-AND-SELF-IMPROVEMENT.md`).
