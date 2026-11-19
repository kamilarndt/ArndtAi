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

Wariant z 100+ eventami (filtr i szukanie):

```sh
python3 app/web/fixture_events.py --out .agent/sessions --name fixture-duza --pad 60
```

albo do dowolnego katalogu, bez wpływu na repo:

```sh
python3 app/web/fixture_events.py --out /tmp/fx
```

Jeśli sesje leżą w `app/.agent/sessions` (tak robi `cargo run` uruchamiany z
`app/`), uruchom serwer w katalogu `app/` i wejdź na `http://127.0.0.1:8765/web/`:

```sh
cd app && python3 -m http.server 8765
```

## Sesja z próbką markdownu (runda 20)

```sh
python3 app/web/fixture_events.py --out /tmp/fx-md --markdown
# albo w repo, pod serwer uruchomiony z app/:
python3 app/web/fixture_events.py --out .agent/sessions --markdown
```

tworzy `fixture-md.jsonl` (9 linii): `task` i `tool_result` z dosłownym `**`,
`thought` z próbką każdego elementu, `thought` z czterema blokami kodu i linią
dłuższą niż 120 znaków, `thought` z niepełnym markdownem, `thought` z blokiem
205 linii (dowód obcięcia) oraz `summary` z markdownem w `reason`.

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
- **Markdown w treści asystenta** (runda 20): `thought` i `summary.reason` są
  renderowane — patrz sekcja niżej.
- **Filtr typów** (checkboxy przy każdym `kind`, z licznikami) i **szukanie
  tekstu** po stronie klienta — bez wielkości liter i bez polskich znaków
  diakrytycznych, trafienia podświetlone (także wewnątrz markdownu).
- **Auto-odświeżanie co 2 s**: nowe linie są dosyłane od ostatniego offsetu.
  Czytane są wyłącznie linie zakończone `\n`; niekompletna końcówka czeka
  w buforze do następnego ticku, więc wpis w trakcie zapisu nie pokazuje śmieci.
  Wcześniejsze eventy zostają, duplikatów nie ma.
- **Pasek stanu**: kroki / narzędzia / odmowy / status — jak w TUI.

## Markdown w treści asystenta (runda 20)

Ten sam zakres co `app/src/markdown.rs` (runda 18), ta sama paleta
(`groknight`), ten sam sposób rozpoznawania bloków: `app/web/markdown.js`.

| element | jak wygląda |
|---|---|
| `#`, `##`, `###` | pogrubiona linia bez hashy (`#fff`; h1/h2 o włos większe — TUI ma jeden stopień pisma) |
| `####`… | pogrubiona, przygaszona (`#6c6c6c`) — jak `markdown.rs` dla poziomów 4+ |
| `**pogrubienie**` | atrybut `font-weight: 700`, kolor `#e1e1e1` |
| `*kursywa*`, `_kursywa_` | atrybut `font-style: italic` (podkreślnik tylko na granicy słowa) |
| `` `kod` `` | tło `#242424` (`bg_highlight`), tekst `#e1e1e1`, ten sam krój |
| `-` / `1.` / `1)` | punktor `•` z wcięciem 2 znaki na poziom; numeracja zostaje z pliku |
| ```` ```lang ```` | ramka `#585858` + etykieta języka w prawym górnym narożniku, kod bez zawijania |
| podświetlanie | `rust`, `json`, `bash`, `toml` — słowa kluczowe `#bb9af7`, stringi `#9ece6a`, liczby `#ff9e64`, komentarze `#585858`; inne języki jednym kolorem |
| `> cytat` | pionowa kreska z lewej (`>>` dokłada drugą), tekst przygaszony |
| `\| a \| b \|` | siatka tabeli, nagłówek pogrubiony z regułą pod nim, kolumny bez zawijania (przewijanie w bok) |
| `---` | linia `——`, jak `markdown::RULE_TEXT` |

**Czego nie renderujemy** — zostaje tekstem: obrazy (`![podpis](adres)` → podpis
plus adres w nawiasie), klikalne linki (`[tekst](adres)` → tekst podkreślony
plus adres w nawiasie, bez klikania), HTML, LaTeX, mermaid. Emoji bez zmian.

Zasady:

1. Markdown **tylko** w `thought` i `summary.reason`. `task` i
   `tool_result.output` są dosłowne — `**` w pliku użytkownika nie znika.
2. Niepełny markdown nie wywraca renderu: brak zamknięcia `**`, `*` albo
   `` ` `` zostawia znaki tak, jak przyszły; blok ``` bez zamknięcia pokazuje
   resztę eventu w ramce (tak samo robi TUI). Żaden błąd parsowania nie może
   zatrzymać dosyłania eventów.
3. Blok kodu dłuższy niż 200 linii jest obcinany z `… (N więcej)`; linia dłuższa
   niż 120 znaków dostaje `… +N znaków`. Liczby i progi są te same co w TUI.
4. Renderowanie nie rusza taila: nowe eventy nadal dopisują się na końcu listy
   (`append`), a niekompletna linia nadal czeka w buforze. Render markdownu
   liczy się per wiersz eventu, przy dopisywaniu — nie przy każdym ticku.
5. Zero zmian w `app/src/` i w TUI; markdown.js nie zna pojęcia eventu — dostaje
   tekst i funkcję, która zamienia fragment na węzeł DOM (dzięki temu szukanie
   podświetla trafienia także w nagłówkach, listach i blokach kodu).

Różnice świadome względem TUI (poza wyglądem samych znaków ramki):

- `summary.reason` jest tu renderowany jako markdown (TUI rysuje to pole
  dosłownie) — bo brief rundy 20 wymienia to pole obok `thought`.
- Ramki kodu przy wąskim panelu tracą boczne krawędzie (kontener ≤ 620 px),
  a etykieta języka wchodzi do środka. W TUI dzieje się to poniżej 40 kolumn.
- Zawijanie: TUI łamie akapity na szerokości terminala, przeglądarka robi to
  sama; kod nigdy się nie zawija w obu.
- Końcówki skrótów: TUI pisze `… (N wiecej)` i `… +N znakow` (ASCII), tutaj
  `… (N więcej)` i `… +N znaków` — tak jak w zleceniu rundy 20. Liczby, progi
  i miejsce obcięcia są identyczne.

## Zrzuty (dowód)

| plik | co pokazuje |
|---|---|
| `app/shots/faza-c-markdown.png` | sesja `fixture-md`, panel eventów 960 px |
| `app/shots/faza-c-markdown-500.png` | ten sam ekran, panel 500 px (`waski` to ta sama klatka) |

## Czego nie robi

- Nie uruchamia zadań, nie steruje agentem, nie dotyka SQLite ani `state.db`.
- Nie zapisuje niczego na dysku (zero POST-ów, zero localStorage).
- Nie ma logowania, wykresów, eksportów, edycji.
- Nie czyta plików spoza `.agent/sessions`: lista sesji powstaje z zawartości
  katalogu, a nazwa z `/` albo `..` jest odrzucana przed zbudowaniem adresu.
- Nie zmienia rdzenia w Rust ani `docs/`.
- Nie ładuje niczego z sieci: żadnego CDN, żadnej biblioteki markdownu.

## Ograniczenia, które warto znać

- `python3 -m http.server` nie obsługuje `Range`, więc przy każdym wzroście
  pliku strona pobiera go w całości i parsuje tylko nową końcówkę. Dla sesji
  rzędu tysięcy linii to nadal tanio; prawdziwy serwer z `Range` (206) obsłuży
  to bajtowo.
- Lista sesji korzysta z automatycznego spisu katalogu (`http.server` go robi).
  Serwer bez spisu katalogu pokaże pustą listę.
- Szukanie liczy po widocznym tekście, więc fraza ze znacznikiem (`**`) nie
  znajdzie pogrubienia — znaczniki znikają w renderze.
- Kolory: tło `#1f1f1f`, tekst `#e6e6e6`, akcent `#ff5fd2`, zieleń `#4ec9b0`,
  czerwień `#f14c4c`, przygaszony `#8a8a8a` — reszta palety (`#ff9e64`,
  `#e0af68`, `#7dcfff`, `#bb9af7`) uzupełnia role z TUI. Markdown używa
  dodatkowo `#242424` (tło kodu inline), `#585858` (ramki) i `#6c6c6c`
  (tekst przygaszony) — wartości 1:1 z `src/tui.rs`.

## Pliki

| plik | rola |
|---|---|
| `index.html` | szkielet: pasek górny, lista sesji, widok sesji, pasek stanu |
| `app.js` | pobieranie spisu, tail po offsecie, render 12 typów, filtr i szukanie |
| `markdown.js` | render markdownu w treści asystenta (nagłówki, listy, kod, cytat, tabela) |
| `style.css` | paleta, układ, wygląd markdownu (w tym wąski wariant ramek) |
| `fixture_events.py` | generator sesji testowej (12 typów; `--pad N` dokłada eventy, `--markdown` daje próbkę markdownu) |
