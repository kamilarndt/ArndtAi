# Runda 19: Faza C — minimalny web UI nad kontraktem eventów

Pierwszy klient inny niż TUI. Cel: sprawdzić, czy z zamrożonego kontraktu
`docs/contracts/events.md` (v1) da się zbudować czytelny ekran bez dotykania rdzenia.

## Zakres (jeden ekran, tylko odczyt)

1. **Lista sesji** — pliki `.agent/sessions/*.jsonl` w workspace, najnowsze
   pierwsze: id pliku (bez `.jsonl`), pierwsze zdanie `task`, status z ostatniego
   `summary` (albo `brak`), czas ostatniego eventu.
2. **Widok sesji** — chronologiczna lista eventów; każdy z 12 typów ma własny
   wygląd: `task`, `thought`, `tool_call`, `tool_result`, `tool_result_cached`,
   `tool_result_skipped`, `denied`, `store`, `store_error`, `error`, `summary`,
   `repair`. `tool_call` pokazuje narzędzie i argumenty, `tool_result` jest
   zielony przy `ok:true` i czerwony przy `ok:false`, `summary` to nagłówek
   z licznikami (`status`, `steps`, `tool_calls`, `denied`, `drift`).
3. **Filtr po `kind`** (checkboxy) i **szukanie tekstu** po stronie klienta.
4. **Auto-odświeżanie co 2 s**: dosyłanie nowych linii od ostatniego offsetu.
   Czytaj **tylko linie zakończone `\n`**; niekompletną końcówkę trzymaj w buforze
   do następnego ticku (proces pisze w trakcie).
5. **Zero zapisów** — UI nic nie zmienia na dysku.

## Czego nie robić

- Nie uruchamiaj zadań, nie steruj agentem, nie dodawaj auth, wykresów, eksportów.
- **Nie zmieniaj rdzenia w Rust** (poza tym, co wymienione niżej): żadnych nowych
  zależności, żadnych zmian w `src/` — poza jednym wyjątkiem, patrz „Statyczny serwer".
- Nie ruszaj `docs/`, kontraktów, bramek, dryfu, allowlisty, TUI.

## Pliki

Wszystko nowe ląduje w `app/web/`:

- `app/web/index.html` — szkielet (dwa panele: lista sesji, widok sesji),
- `app/web/app.js` — fetch + tail + render + filtr,
- `app/web/style.css` — paleta z TUI (`groknight`): tło `#1f1f1f`, tekst `#e6e6e6`,
  akcent `#ff5fd2`, zielony `#4ec9b0`, czerwony `#f14c4c`, przygaszony `#8a8a8a`,
- `app/web/fixture_events.py` — generator JSONL z **jednym eventem każdego
  z 12 `kind`** (`python3 app/web/fixture_events.py --out <katalog>` tworzy
  `<katalog>/fixture-12.jsonl`),
- `app/web/README.md` — jak uruchomić (jedna komenda), czego nie robi.

## Jak to się serwuje

Strona jest statyczna; serwujemy ją z **korzenia workspace'u**, bo tam leży
`.agent/sessions`:

```sh
cd <workspace> && python3 -m http.server 8765
# otwórz http://127.0.0.1:8765/app/web/
```

`fetch('/.agent/sessions/<id>.jsonl')` działa, bo origin jest ten sam.
**Nie dodawaj serwera w Rust** — żadnych nowych zależności.

## Kryteria PASS (odbieram to sam)

1. `cd app && cargo build --quiet && cargo test --quiet` → exit 0; `cargo run
   --quiet -- demo` → 39/39. Rdzeń nietknięty.
2. `python3 app/web/fixture_events.py --out /tmp/fx` tworzy plik z 12 liniami,
   po jednym `kind` każdym; UI pokazuje wszystkie 12 wariantów **bez błędu
   w konsoli** (Chromium i Firefox).
3. Filtr `kind` ukrywa i pokazuje właściwe wiersze; szukanie tekstu działa na
   sesji z 100+ eventami (bez progu czasu — to cel, nie bramka).
4. Auto-odświeżanie: dopisanie linii do pliku dokłada ją bez przeładowania
   i bez duplikatów; wcześniej zapisany event nie znika.
5. Częściowa linia: dopisanie JSON-a **bez `\n`** nie wywala UI i nie pokazuje
   śmieci; po dopisaniu `\n` event pojawia się w następnym ticku.
6. Ścieżka z `..` w `fetch` jest odrzucana po stronie klienta (lista sesji
   budowana z whitelisty plików, nie ze ścieżki użytkownika).
7. Brak zmian w `app/src/` i `docs/` (`git status --short` pokazuje tylko
   `app/web/`).

## Gotowe gdy

- Punkty 1–7 powyżej są spełnione, a `app/web/README.md` mówi, jak otworzyć UI.
- Nie ma zrzutów ekranu w tej rundzie — odbiór robię w Orca browser i sam
  dopiszę zrzuty do `app/shots/`.
