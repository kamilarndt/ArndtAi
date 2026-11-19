# Faza C — minimalny web UI nad kontraktem eventów

Status: spec do akceptacji. Data: 2026-11-19. Właściciel decyzji: Kamil.

## Cel

Drugi klient kontraktu `docs/contracts/events.md`: przeglądarka czyta JSONL
sesji i pokazuje przebieg zadania bez uruchamiania rdzenia. Ma to zweryfikować
kontrakt (czy da się z niego zbudować UI) i dać podgląd sesji zdalnych.

## Zakres (jeden ekran)

- Lista sesji z `.agent/sessions/*.jsonl` (najnowsze pierwsze): nazwa pliku,
  pierwsze zdanie `task`, status z ostatniego `summary`, czas.
- Widok sesji: chronologiczna lista eventów, każdy typ ma własny wygląd
  (`thought` = szary, `tool_call` = z argumentami, `tool_result` = zielony/czerwony,
  `denied` = pomarańczowy, `summary` = nagłówek z licznikami).
- Filtr po `kind` i pole szukania tekstu (po stronie klienta).
- Auto-odświeżanie co 2 s, gdy plik rośnie (tail od ostatniego offsetu).
  Czytamy wyłącznie linie zakończone `\n`; niekompletną końcówkę trzymamy
  w buforze do następnego ticku (proces pisze w trakcie).
- Zero zapisów: UI jest tylko do czytania.

## Poza zakresem

- Uruchamianie zadań z UI, sterowanie agentem, edycja czegokolwiek.
- Uwierzytelnianie i praca zdalna (tylko `file://`/lokalny serwer statyczny).
- Wykresy, statystyki, eksporty.

## Architektura

Statyczna strona (HTML + JS, bez frameworka i bez build-stepu), serwowana
lokalnie przez `python3 -m http.server` z korzenia workspace'u. Czyta pliki
przez `fetch('/.agent/sessions/<id>.jsonl')`. Rdzeń (`app/`) bez zmian —
żadnych nowych zależności w Rust.

## Kryteria PASS

1. `cd app && cargo test` → exit 0 (kontrakt i rdzeń nietknięte).
2. Strona otwiera się bez błędów w konsoli na Chromium i Firefox.
3. `scripts/fixture_events.py --out <dir>` generuje JSONL z dokładnie jednym
   eventem każdego z 12 `kind`; UI pokazuje wszystkie 12 wariantów bez błędu
   w konsoli. Nie opieramy tego na sesji demo, która może nie mieć `repair`
   ani `tool_result_skipped`.
4. Filtr `kind` i szukanie działają na sesji z 100+ eventami; brak progu czasu
   w bramce (wydajność to cel, nie kryterium odbioru).
5. Auto-odświeżanie dokłada nowe linie bez przeładowania i bez duplikatów.
6. Brak dostępu do plików spoza `.agent/sessions` (ścieżka z `..` odrzucona).
6a. Częściowa linia: dopisanie do pliku JSON-a bez `\n` nie wywala UI i nie
   pokazuje śmieci; po dopisaniu `\n` event pojawia się w następnym ticku.
7. Zrzuty: lista sesji, widok sesji, filtr aktywny — w `app/shots/faza-c-*.png`.

## Ryzyka

- Format JSONL jest zamrożony (v1) — UI nie może wymagać zmian w rdzeniu;
  jeśli czegoś brakuje, to nowa wersja kontraktu, nie cicha zmiana.
- Duże sesje (tysiące linii) — potrzebny tail przyrostowy, nie pełny re-parse.
- `state.db` nie jest czytane przez UI w tej fazie (tylko JSONL), więc statusy
  kroków liczone są z eventów.
