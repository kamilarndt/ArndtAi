# 07 — Reverse-Engineering Mandate

**Zasada nadrzędna:**  
Zanim zaimplementujesz jakikolwiek większy mechanizm, rozbierz co najmniej 2–3 istniejące systemy, które rozwiązują podobny problem. Weź wzorce, nie kopiuj kodu.

## Priorytetowe cele do rozebrania

| Mechanizm | Rozbierz | Weź |
|-----------|----------|-----|
| Learning loop + skills + memory + cron | **Hermes Agent** (Nous Research) | Tworzenie/poprawianie skills, MEMORY.md/USER.md, FTS5, scheduled automations, subagents |
| Gateway + multi-channel + automations | **OpenClaw** | Control plane, channels (WhatsApp/Telegram/Slack…), skills, local-first |
| Multi-mode agents + marketplace | **Kilo Code** | Code/Plan/Debug/Ask, skills/MCP marketplace, cloud agents |
| Minimal core + extensions | **pi** (earendil-works) | 4-tool core, TypeScript self-extension, multi-provider |
| Companion (always-on, skills, proaktywność) | Hello Minds | Soul, Skills jako playbooki, Circles, kumulująca się pamięć |
| Pamięć Lite/Pro/Ultra | HyAtlas-Memory | Warstwy, tryby, konsolidacja |
| Single-writer + outbox + proaktywność | dsh-superagent | Transakcje, inbox, sesja-per-zadanie |
| Format osobowości | SillyTavern | Pola karty, World Info, token control (jako technika) |

## Jak rozbierać
- Używaj narzędzi inspekcji Craft-Anything.
- Zapisuj wnioski (własnymi słowami + lokalizatory).
- Porównuj 2–3 implementacje zanim wybierzesz.
- Dokumentuj decyzję: co wzięto, co odrzucono i dlaczego.

## Zakaz
Nie implementuj „z głowy”, jeśli istnieje działający wzorzec w Hermesie, dsh, HyAtlas lub SillyTavern.
