# 06 — Proactivity and Self-Improvement

## Proaktywność
- Default-deny.
- Perimeter pisze surowe zdarzenia do `trigger_inbox`.
- Reguły w `policies/*.yaml` (git).
- Brak reguły = log i porzucenie.
- Nowa reguła 14 dni w shadow.
- Sesja `worker-<task_id>`, limit równoległości.
- Budżet uwagi, godziny ciszy.

## Self-improvement
Tylko deklaratywne artefakty:
- `personality/base.md`
- `skills/`
- `policies/`
- opisy narzędzi

Job analizuje transkrypty → proponuje diff → eval gate (Pass^k, holdout, canary, auto-rollback).

## Reverse-engineering mandate
Rozbierz proaktywność i polityki w dsh-superagent oraz mechanizmy self-improvement / eval w innych systemach agentowych.
