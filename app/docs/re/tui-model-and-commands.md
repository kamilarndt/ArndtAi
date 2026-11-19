# Runda 14: metadane modelu, picker `/model`, rejestr komend `/`

Data: 2026-11-14. Zakres: licznik kontekstu z `limit.context` zamiast sztywnego
`500K`, modal `/model`, pierwsza infrastruktura komend. Kontrakty
(`docs/contracts/`), bramki, dryf, allowlista i perimetr bez zmian; żadnego web
UI ani markdownu.

## 1. Skąd biorą się metadane

Provider OpenAI-compatible wystawia `GET {base_url}/models`
(Anthropic: `GET {base_url}/v1/models`). Odpowiedź, na której opiera się
parser (`src/models.rs::parse_models`):

```json
{"data": [{"id": "mimo-v2.6-flash",
           "limit": {"context": 1048576, "output": 131072},
           "cost": {"input": 0.05, "output": 0.2},
           "capabilities": {"tools": {"supported": true}}}]}
```

Obsługiwane kształty: `{"data": [...]}` (OpenAI), `{"models": [...]}` i goła
tablica. Wiersze bez `id` są pomijane, duplikaty `id` — tylko pierwszy.
`limit` jest opcjonalny: brak `limit` to brak limitu (`?`), nie domyślne 200K.
`capabilities` nie jest jeszcze używane — zostaje na rundę z filtrem modeli,
które nie umieją narzędzi.

Przepływ:

1. `App::new` (`src/tui.rs`) woła `models::load(workspace, conn)` **raz na
   start procesu**. `conn` (`provider::Brain::conn`) jest `None` w trybie
   offline (mock), więc sieci nie ma czego pytać.
2. `models::load`: świeży cache (`.agent/models.json`, TTL 1 h) → `GET /models`
   (`curl --max-time 5`) → przeterminowany cache (`Source::Stale`) → pusto
   (`Source::Missing`, licznik pokaże `?`).
3. Pobranie zapisuje cache w tym samym kształcie co odpowiedź providera
   (`{"fetched_ms": …, "data": […]}`), żeby parser czytał jedno i drugie.
4. W testach jednostkowych sieć jest wyłączona (`cfg!(test)`) — testy nie
   zależą od maszyny.

Nagłówki (w tym klucz API) idą do pliku `--config` curl 0600, nie do `argv` —
ta sama reguła co przy `POST /chat/completions`.

## 2. Licznik kontekstu

Prawy górny róg: `{zużyte} / {limit.context}`.

- `zużyte` — `usage` ostatniej odpowiedzi (`prompt_tokens` + `completion_tokens`
  u OpenAI, `input_tokens` + `output_tokens` u Anthropica). Bierzemy **ostatnią**
  odpowiedź, nie sumę: prompt już zawiera całą historię, więc sumowanie
  liczyłoby ją wielokrotnie. `usage` ląduje w `provider::{note_usage,
  last_usage}` i TUI czyta je w `poll()`.
- Provider nie przysyła `usage` → `zużyte` liczone z znaków transkryptu
  (`count/4`, zaokrąglenie w górę) z jawnym `~`: `~1.5K / 1.0M`.
- Limit nieznany → `1.5K / ?` (albo `~1.5K / ?`). Kolor licznika zmienia się
  przy 70% i 90% limitu (żółty/czerwony); bez limitu zostaje szary.
- Etykiety: progi binarne, jedna cyfra po kropce — `1.5K`, `128.0K`, `1.0M`
  (`models::tokens_label`). `500K` znika z kodu.
- `limit.output` jest domyślnym `max_tokens`, ale przyciętym do `4096`
  (`models::Model::output` → `Config::max_tokens` → `RemoteBrain.max_tokens`).
  Runda 15 zamieniła sztywne `4096` na ustawienie `max_tokens`
  (`1024|2048|4096|8192`) — patrz `app/docs/re/settings.md`.
- Nowa sesja (`open_new_session`, wejście w transkrypt) zeruje licznik i
  `provider::reset_usage()` — `usage` poprzedniej sesji nie może zostać.

## 3. `/model` — picker

- Modal w stylu „Resume session": pełna ramka, pasek tytułu `Select model`,
  lista, stopka klawiszy (`src/welcome.rs::picker`). Wiersz: `●`/`○` (aktywny),
  nazwa, `id`, okno kontekstu, cena `input / output` za 1M (`—`, gdy provider
  jej nie podaje).
- `↑/↓` wybór, `Enter` przełącza model **w bieżącej sesji**, `Esc` zamyka.
- Wybór trafia do `.agent/config` jako `model = <id>` (format `key = value`;
  zapis nie rusza pozostałych kluczy). Precedencja:
  `ARNDTAI_MODEL` → `.agent/config` → domyślny model providera
  (`models::resolve_model`; sam wybór modelu w providerze nadal sprawdza
  `ARNDTAI_MODEL` jako pierwszy).
- Zmiana dopisuje do transkryptu sesji `{"kind": "note", "data": {"model":
  "<id>"}}` (`context::Session::resume(...).event("note", …)`). Gdy sesji
  jeszcze nie ma (przełączenie przed pierwszym zadaniem), `note` czeka w
  `pending_note` i idzie do pliku zaraz po pierwszym przebiegu.
- Bez listy modeli `/model` nie otwiera pustego modala — pisze w scrollbacku,
  że brak metadanych i zostaje przy obecnym modelu.
- Lista w zrzucie (`models::demo_catalog`) jest statyczna i **bez cen**:
  offline nie ma providera, więc zmyślona cena byłaby nieprawdą.

## 4. Rejestr komend

`src/commands.rs` — jedno miejsce: `name`, `about`, `needs_arg`, `handler`
(`Handler`: `Help`, `Model`, `Settings`, `Clear`, `Resume`, `Quit`). Startowe
sześć: `/help`, `/model`, `/settings` (komunikat „w kolejnej rundzie"),
`/clear`, `/resume`, `/quit`. Kolejna runda dopisuje wiersze tutaj, a TUI
tłumaczy `Handler` na akcję — nie ma drugiej listy w `tui.rs`.

Zachowanie w composerze:

- tekst zaczynający się od `/` (bez spacji) → lista podpowiedzi **pod
  composerem**, nad stopką; wiersze na listę bierze scrollback
  (`Screen::reserve_hints`), więc nic nie jest zasłaniane,
- `Tab` i `Enter` uzupełniają do zaznaczonej komendy, `↑/↓` chodzą po liście,
- `Esc` chowa podpowiedzi (drugi `Esc` wraca do listy sesji),
- dokładnie wpisana komenda + `Enter` uruchamia handler; nieznana nazwa daje
  `nieznana komenda /x — wpisz /help` w scrollbacku i `toast`, bez paniki,
- `/` działa też z ekranu powitalnego (`/help`, `/resume`, `/quit`),
- `/help` wypisuje tabelę w jednym wpisie: `Komendy:` + wyrównane
  `nazwa` + opis (dwie spacje wcięcia, dopełnienie do najdłuższej nazwy).

## 5. Zrzuty

`--screenshot` (sesja z licznikiem), `--screenshot-model` (modal `/model`),
`--screenshot-hints` (podpowiedzi `/`): 120×38 znaków = 960×608 px,
`app/shots/r14-session.png`, `r14-model.png`, `r14-hints.png`.

## Odstępstwa i granice

- Nazwa modelu jest w stopce composera (`mimo-v2.6-flash · always-approve`),
  a nie w górnym pasku — pasek został w układzie z wcześniejszych rund
  (ścieżka + licznik, bez modelu) i runda 14 go nie zmienia.
- Podpowiedzi `/` leżą pod composerem (nad stopką) i zabierają wiersze
  scrollbackowi, więc nie zasłaniają transkryptu; composer przesuwa się przy
  tym o liczbę podpowiedzi w górę.
- Cena w pickerze to `cost.input / cost.output` providera traktowane jako USD
  za 1M tokenów — tak podaje OC-Go/models.dev. Inna waluta albo inna jednostka
  wymaga potwierdzenia w metadanych, więc na razie nie jest przeliczana.
- Lista w zrzucie jest statyczna i bez cen (offline nie ma providera).
- Streaming `usage` (SSE) nie istnieje, bo pętla nie streamuje odpowiedzi.

## 6. Co zostaje na kolejne rundy

- `/settings`: realne ustawienia (limit wyjścia, tryb zgód), nie komunikat.
- Filtr listy modeli po `capabilities.tools` i obsługa `cost` w innej walucie.
- Odświeżanie listy w locie (`Ctrl+R` w pickerze) i `--refresh-models`.
- `usage` strumieniowe (SSE) — teraz tylko odpowiedź bez streamingu.
- Argumenty komend (`needs_arg` jest w rejestrze, ale żadna startowa komenda
  go nie używa; parsing argumentów przyjdzie z `/settings`).
- Wyszukiwanie w `/model` przy długich listach.
