# Eval gate

`ardntai eval [--k N]` uruchamia każdy przypadek z `evals/cases/*.json`
N razy w czystym workspace i liczy **Pass^k** (wszystkie powtórzenia muszą
przejść). Provider jest offline (mock), więc bramka jest deterministyczna
i nie kosztuje tokenów.

Pola przypadku:

| pole | znaczenie |
|---|---|
| `name` | nazwa przypadku (katalog roboczy) |
| `task` | treść zadania |
| `files` | pliki wejściowe: nazwa → treść |
| `expect` | ścieżka → fragment treści, który musi wystąpić |
| `status` | oczekiwany status (domyślnie `DONE`) |

`ardntai improve --apply --gate` przepuszcza kandydującą regułę przez ten
zestaw i **wycofuje ją** (snapshot `policies/`), gdy cokolwiek nie przejdzie —
tak działa auto-rollback. Brak przypadków = brak dowodu = zmiana odrzucona.
