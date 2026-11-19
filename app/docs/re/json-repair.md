# Ucięty JSON nie zabija zadania (runda 12)

Data: 2026-11-14. Zakres: `app/src/provider.rs`, `app/src/agent.rs`,
`app/tests/cli.rs`, ten dokument. Kontrakty, TUI, schemat bazy, bramki, dryf
i allowlista bez zmian.

## 1. Objaw i przyczyna

Test na żywo (`ws-02b-refusal`) kończył się `FAILED_ERROR`, choć model po prostu
odpowiedział uciętym JSON-em:

```
✗ brak konca JSON-a
FAILED_ERROR · kroki 1 · narzedzia 0 · odmowy 0 · dryf 0
```

Dwie przyczyny, obie w `provider.rs`:

1. `max_tokens: 1200` w obu dostawcach. Przy dłuższym `thought` albo `args`
   model urywał obiekt w połowie — odpowiedź była poprawna merytorycznie, ale
   niekompletna składniowo.
2. `parse_decision` brał ostatni `}` i wymagał poprawnego obiektu. Błąd
   parsowania kończył całe zadanie bez ani jednej próby naprawy.

## 2. Co robi naprawa

- `MAX_TOKENS = 4096` w obu miejscach (Anthropic i OpenAI-compatible).
- Pierwsze żądanie idzie bez zmian. Jeśli `parse_decision` zwróci błąd,
  wysyłane jest **jedno** żądanie naprawcze: to samo zadanie, ta sama historia,
  na końcu dopisana instrukcja `REPAIR_INSTRUCTION` — „zwróć wyłącznie poprawny
  JSON, bez prozy i bez markdown” plus przypomnienie obu dozwolonych kształtów
  odpowiedzi.
- Druga odpowiedź parsuje się poprawnie → krok leci dalej normalnie
  (`DONE`/`FAILED_*` według pętli). Druga też nie parsuje → `FAILED_ERROR`.
  Dokładnie jeden retry, bez pętli.
- Błąd transportu (curl, timeout, brak treści) nie jest błędem parsowania
  i nie uruchamia naprawy — nie ma czego naprawiać.
- `finish`/`tool`/`args` czyta ten sam `parse_decision`; naprawa nie zmienia
  semantyki decyzji, tylko daje modelowi drugą szansę na poprawny JSON.

## 3. Ile to kosztuje

Jedno dodatkowe żądanie do modelu **tylko wtedy**, gdy pierwsza odpowiedź się
nie parsuje. Poprawna odpowiedź za pierwszym razem to nadal jedno żądanie
(pilnuje tego test `valid_answer_costs_exactly_one_request`). Limit tokenów
4096 zamiast 1200 nie zwiększa liczby żądań, tylko maksymalną długość
odpowiedzi — koszt rośnie dopiero wtedy, gdy model faktycznie pisze dłużej.

## 4. Diagnostyka

Komunikat błędu ma postać:

```
<blad parsowania 2. proby> — proba 2/2 (po naprawie); proba 1: <blad 1. proby>;
ogon odpowiedzi: ...<ostatnie 160 znakow surowej odpowiedzi>
```

Ta sama treść trafia do:

- JSONL sesji jako `{"kind":"error","data":{"message":...}}` — bez zmiany
  kształtu istniejącego zdarzenia `error`,
- statusu sesji (`summary.reason`, `Status:`/`Powod:` na wyjściu,
  `tasks.status` w SQLite przez istniejącą ścieżkę).

Nowe zdarzenie `repair` (`data`: `step`, `detail`) pojawia się przy każdej
próbie naprawy — także wtedy, gdy zadanie kończy się `DONE`. To jedyny
dodatek do kanału zdarzeń; wymaga jednej linii w `docs/contracts/events.md`
(„`repair` | próba naprawy odpowiedzi modelu | `step`, `detail`”), którą
uzupełnia właściciel dokumentu.

## 5. Czego naprawa nie robi

- Nie zgaduje brakujących pól. Ucięty `{"tool":"fs.write","args":{"path":"a` nie
  zostanie „dokończony” po stronie agenta — model dostaje drugie zadanie
  i musi sam zwrócić komplet.
- Nie powtarza żądania po błędzie sieci, nie zmienia `FAILED_*`, nie rusza
  bramek (allowlista, scope guard, dryf) ani allowlisty.
- Nie zmienia formatu `mcp.md`/`events.md` poza nowym `repair`.
- Nie zmienia TUI: naprawa jest widoczna w scrollbacku jako linia `repair`
  (nowe zdarzenie kanału), a status końcowy liczy się jak dotąd.

## 6. Dowody

- `cargo build` — exit 0, bez ostrzeżeń (usunięty nieużywany import w teście
  `welcome.rs`).
- `cargo test` — 116 testów jednostkowych i 7 integracyjnych, wszystkie zielone.
  Nowe jednostkowe: `max_tokens_leaves_room_for_a_whole_answer`,
  `valid_answer_costs_exactly_one_request`,
  `truncated_answer_is_repaired_once_and_the_task_goes_on`,
  `two_truncated_answers_fail_with_the_tail_of_the_raw_answer`,
  `tail_shows_the_end_not_the_beginning`,
  `transport_error_on_the_first_request_is_not_retried`.
  Nowe integracyjne (prawdziwa binarka + serwer HTTP na localhost, który liczy
  żądania): `truncated_answer_is_repaired_and_the_task_finishes`,
  `valid_first_answer_does_not_cost_a_second_request`,
  `two_truncated_answers_fail_with_the_tail_of_the_answer`.
- `cargo run -- demo` — 39/39, `DEMO OK`.
- Mock offline bez zmian: `Brain::Mock` nie ma prób naprawy
  (`take_repairs` zwraca pusty wektor).
