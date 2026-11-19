# 05 — Loop and Gates

## Stany pętli
`RECEIVED → PLANNING → STEP_PROPOSED → SCOPE_CHECK → POLICY_CHECK → EXECUTING → OBSERVE → STOP_CHECK`  
(+ stany końcowe: DONE, FAILED_BUDGET, FAILED_STALLED, WAITING_APPROVAL, WAITING_PERIPHERY)

Jedno przejście = jedna transakcja.

## Bramki (przed każdym krokiem)
1. Budżet
2. Allowlist narzędzi
3. Strażnik zakresu
4. Detektor dryfu

## Reverse-engineering mandate
Rozbierz pętlę Hermesa i mechanizmy stopu / bramek w dsh-superagent.  
Weź najlepsze elementy kontroli.
