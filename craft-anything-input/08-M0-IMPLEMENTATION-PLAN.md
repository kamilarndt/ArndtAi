# 08 — M0 Implementation Plan

## Cel M0
Działający minimalny rdzeń: pętla + 2 bramki + MCP + `base.md` (Lite) + jeden skill folder.

## Zakres
- Rdzeń Python z pętlą stanów.
- Bramki: budżet + allowlist.
- MCP client (stdio) + perimeter z 3 narzędziami (`fs.read`, `fs.write`, `notify.send`).
- Prefiks z `personality/base.md`.
- Pamięć Lite (raw log).
- Jeden skill folder.
- SQLite: tasks, sessions, transcripts.

## Kroki
1. Szkielet pętli.
2. SQLite + single-writer.
3. Składanie prefiksu z base.md.
4. MCP client.
5. Perimeter TS.
6. Bramki.
7. Skill loader.
8. Golden testy (5 zadań).

## Kryterium wyjścia
Agent w Lite przyjmuje zadanie, wykonuje 2–3 kroki przez MCP i kończy poprawnie.

## Reverse-engineering przed kodem
Rozbierz Hermes MCP client i prostą pętlę zanim napiszesz własną.
