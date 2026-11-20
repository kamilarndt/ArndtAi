# Rozbiórki (REA) wymagane przez spec — stan

Spec (`07-REVERSE-ENGINEERING-MANDATE.md` i sekcje „mandate" w 02–06) wymaga
rozbiórki wzorców **przed** implementacją danej warstwy. Ten plik mówi, co jest,
a czego nie ma — żeby brak nie wyglądał na zrobione.

| Wzorzec | Wymagany przez | Stan | Notatka |
|---|---|---|---|
| grok-build (TUI, pager, slash) | `NEXT.md`, runda 14-18 | DONE | `docs/re/grok-build-tui.md`, `tui-parity.md` |
| HyAtlas-Memory (L1–L8, tryby, konsolidacja) | `02:17-19` | BRAK | brak dostępu do źródeł w tej maszynie; pamięć zbudowana z opisu w specu |
| Hello Minds (osobowość, playbooki) | `03:19-22`, `04:14-19` | BRAK | j.w.; format `personality/base.md` jest własny, nie z karty |
| SillyTavern (karty postaci, lorebook) | `03:19-22` | BRAK | j.w. |
| dsh-superagent (polityki, eval) | `06:21-22` | BRAK | j.w.; proaktywność zbudowana z opisu w specu |
| Hermes skills | `04:14-19` | CZĘŚĆ | loader `SKILL.md` z pola `name/description/when`; format skills z tego repo (`skills/przyklad`) |

## Dlaczego brak

Wzorce są zamknięte albo niedostępne lokalnie. Zamiast zgadywać ich
wnętrzności, implementacja idzie za wymaganiami w specu, a różnice są
odnotowane w ADR-ach i w `docs/STATUS.md`. Gdy źródła będą dostępne, rozbiórki
wchodzą jako osobne notatki — bez zmiany kodu, który już działa.
