# Podgląd sesji — web UI (Faza C)

Statyczna strona czytająca zamrożony kontrakt `docs/contracts/events.md` (v1):
pliki `.agent/sessions/<id>.jsonl`, jedna linia = jeden event.

Bez frameworka, bez build-stepu, bez nowych zależności w Rust. Tylko odczyt.

## Jak otworzyć (jedna komenda)

Z korzenia projektu (tam leży `.agent/sessions`):

```sh
python3 -m http.server 8765
```

i otwórz <http://127.0.0.1:8765/app/web/>.

Inny port jest w porządku — ważne, żeby serwer startował z korzenia workspace'u,
bo strona pobiera `/.agent/sessions/<id>.jsonl` z tego samego originu.

## Sesja testowa z 12 typami eventów

```sh
python3 app/web/fixture_events.py --out .agent/sessions
```

tworzy `.agent/sessions/fixture-12.jsonl` — 12 linii, po jednym evencie każdego
typu (`.agent/` jest w `.gitignore`, więc repo zostaje czyste). Odśwież stronę:
sesja pojawi się na liście i pokaże wszystkie 12 wariantów.

Wariant z 100+ eventami ( filtr i szukanie):

```sh
python3 app/web/fixture_events.py --out .agent/sessions --name fixture-duza --pad 60
```

albo do dowolnego katalogu, bez wpływu na repo:

```sh
python3 app/web/fixture_events.py --out /tmp/fx
```

## Co robi

- **Lista sesji** z `.agent/sessions/*.jsonl`, najnowsze pierwsze: id pliku,
  pierwsze zdanie `task`, status z ostatniego `summary` (albo `brak`), czas
  ostatniego eventu.
- **Widok sesji**: chronologiczna lista eventów; każdy z 12 typów ma własny
  wygląd — `task` jako ramka zadania, `thought` przygaszony, `tool_call`
  z narzędziem i argumentami, `tool_result` zielony przy `ok:true` i czerwony
  przy `ok:false`, `tool_result_cached` / `tool_result_skipped` jako warianty
  wyniku, `denied` pomarańczowy, `store` / `store_error` jako stan magazynu,
  `error` czerwony, `summary` jako nagłówek z licznikami (`status`, `steps`,
  `tool_calls`, `denied`, `drift`), `repair` żółty.
- **Filtr typów** (checkboxy przy każdym `kind`, z licznikami) i **szukanie
  tekstu** po stronie klienta — bez wielkości liter i bez polskich znaków
  diakrytycznych, trafienia podświetlone.
- **Auto-odświeżanie co 2 s**: nowe linie są dosyłane od ostatniego offsetu.
  Czytane są wyłącznie linie zakończone `\n`; niekompletna końcówka czeka
  w buforze do następnego ticku, więc wpis w trakcie zapisu nie pokazuje śmieci.
  Wcześniejsze eventy zostają, duplikatów nie ma.
- **Pasek stanu**: kroki / narzędzia / odmowy / status — jak w TUI.

## Czego nie robi

- Nie uruchamia zadań, nie steruje agentem, nie dotyka SQLite ani `state.db`.
- Nie zapisuje niczego na dysku (zero POST-ów, zero localStorage).
- Nie ma logowania, wykresów, eksportów, edycji.
- Nie czyta plików spoza `.agent/sessions`: lista sesji powstaje z zawartości
  katalogu, a nazwa z `/` albo `..` jest odrzucana przed zbudowaniem adresu.
- Nie zmienia rdzenia w Rust ani `docs/`.

## Ograniczenia, które warto znać

- `python3 -m http.server` nie obsługuje `Range`, więc przy każdym wzroście
  pliku strona pobiera go w całości i parsuje tylko nową końcówkę. Dla sesji
  rzędu tysięcy linii to nadal tanio; prawdziwy serwer z `Range` (206) obsłuży
  to bajtowo.
- Lista sesji korzysta z automatycznego spisu katalogu (`http.server` go robi).
  Serwer bez spisu katalogu pokaże pustą listę.
- Kolory: tło `#1f1f1f`, tekst `#e6e6e6`, akcent `#ff5fd2`, zieleń `#4ec9b0`,
  czerwień `#f14c4c`, przygaszony `#8a8a8a` — reszta palety (`#ff9e64`,
  `#e0af68`, `#7dcfff`, `#bb9af7`) uzupełnia role z TUI.

## Pliki

| plik | rola |
|---|---|
| `index.html` | szkielet: pasek górny, lista sesji, widok sesji, pasek stanu |
| `app.js` | pobieranie spisu, tail po offsecie, render 12 typów, filtr i szukanie |
| `style.css` | paleta i układ |
| `fixture_events.py` | generator sesji testowej (12 typów; `--pad N` dokłada eventy) |
