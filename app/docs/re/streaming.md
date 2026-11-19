# Streaming tokenów z providera do TUI (runda 21)

Stan: wdrożone 2026-11-19. Zakres: `provider.rs`, `agent.rs`, `tui.rs`; bez zmian
kontraktów (`docs/contracts/`), bramek, allowlisty, schematu bazy i web UI.

## Po co

Do rundy 20 `provider.rs` wysyłał `stream: false` i czekał na cały JSON. TUI
umiał już rosnący blok myśli (runda 18), ale nie miał czego rysować: model
milczał do końca kroku. Teraz `thought` pisze się na żywo, a krok kończy się
dopiero, gdy przyjdzie poprawny JSON decyzji.

## Format SSE

Żądanie: `"stream": true` w obu ścieżkach (Anthropic i OpenAI-compatible).
Transport to nadal `curl`, ale z `-N` (bez buforowania stdout) i czytaniem
linia po linii:

```
data: {"choices":[{"delta":{"reasoning":"Myślę"}}]}
data: {"choices":[{"delta":{"content":"{\"thought\":\"..."}}]}
data: [DONE]
```

- OpenAI-compatible (OC-Go, DeepSeek): `choices[0].delta.reasoning`
  (czasem `reasoning_content`) to myślenie, `choices[0].delta.content` to
  odpowiedź. `data: [DONE]` zamyka strumień.
- Anthropic: `content_block_delta` z `delta.type = thinking_delta` (pole
  `thinking`) albo `text_delta` (pole `text`); `message_stop` zamyka strumień.
- Nagłówek `x-opencode-session: <session_id>` jest w każdym żądaniu
  OpenAI-compatible — bez niego OC-Go zwraca `MissingSessionID`. Wartość bierze
  się z `ARNDTAI_SESSION` albo jest losowana raz na proces. Klucz i sesja
  siedzą w pliku `--config` curl (0600), nigdy w argv.

Śmieci (komentarze SSE `:`, linie `event:`, puste linie, nie-JSON) są
pomijane. Pojedynczy zły chunk nie kończy kroku; rozłączenie rozpoznaje
warstwa czytająca `curl` (EOF z błędem transferu / niezerowy exit).

## Co robimy z `reasoning`

- Sklejamy osobno od `content`; `content` jest wynikiem funkcji (pełny JSON
  decyzji, jak dotąd), więc pętla i mechanizm `repair` z rundy 12 działają bez
  zmian.
- Przyrosty idą do TUI osobnym kanałem `provider::Delta { step, channel, text }`
  (globalny sink, wzorzec `context::set_event_sink`). **Nie** zapisujemy ich do
  JSONL: kontrakt `events.md` zostaje zamknięty, a transkrypt nie puchnie od
  tysięcy linii. Po zakończeniu kroku przychodzi normalne zdarzenie `thought`
  i blok renderuje się jak dotąd (markdown).
- TUI dopisuje przyrost do **ostatniego** bloku `Thought` z tym samym krokiem
  (`live`), zamiast dopisywać nowy wpis — brak migotania i przeskoku
  scrollbacku. Zdarzenie `thought` zamyka blok w miejscu; przy błędzie tury
  znacznik `live` gaśnie w `finish_report`.

## Koszt

- Jedno żądanie HTTP jest dłużej otwarte (tyle, ile trwa generacja). Limit
  `--max-time` z `ARNDTAI_TIMEOUT` (domyślnie 300 s) nadal obowiązuje cały
  transfer, nie przerwę między chunkami.
- `curl` zostaje jedynym transportem — zero nowych zależności.
- Po stronie TUI koszt to jedna aktualizacja ostatniego bloku na przyrost;
  markdown parsuje się od nowa tylko dla tego bloku (cache z rundy 18 kluczuje
  po treści i szerokości).
- Przy zerwanym strumieniu: jedno ponowienie **bez** strumienia. Jeśli ono
  też padnie, krok kończy się `FAILED_ERROR`, a komunikat niesie oba powody
  (mechanizm ogona z rundy 12).
- Ponowienie nie powtarza efektu: przed `proposed` nie ma jeszcze żadnego
  wywołania narzędzia, a `(session_id, step_id)` zostaje ten sam.

## Czego nie robimy

- Nie streamujemy do web UI (Faza C) — to osobna runda; tam eventy nadal
  przychodzą z JSONL po pełnych liniach.
- Nie zapisujemy `reasoning` do transkryptu ani SQLite (kontrakt bez zmian;
  podgląd żyje tylko w TUI).
- Nie zmieniamy schematu decyzji: nadal jeden obiekt JSON
  `{thought, tool|finish, args}`.
- Nie dodajemy reconnectu w środku strumienia: jest tylko jedno ponowienie
  bez strumienia, żeby nie dublować kosztu i nie gubić idempotencji.

## Dowody

- Testy jednostkowe `provider::tests` na atrapie SSE: 3×`reasoning` + 2×
  `content` = 5 przyrostów i sklejone `content`; brak callbacku = brak
  przyrostów; `[DONE]` kończy parsowanie; śmieci nie wywalają; rozłączenie →
  retry bez strumienia; oba transporty padają → komunikat z dwoma powodami.
- Testy TUI: `streamed_reasoning_grows_one_live_block_and_closes_it`,
  `stream_screenshot_shows_the_live_thought_block`.
- Sprawdzenie end-to-end na lokalnym serwerze SSE (bez sieci): pierwsze żądanie
  `stream:true` z `x-opencode-session`, wynik `DONE`; wariant z urwanym
  transferem: drugie żądanie `stream:false`, zdarzenie `repair`, `DONE`.
- Zrzut: `app/shots/v0.2-stream.png` (blok `◆ Thought · streaming` w trakcie
  pisania, wiersz `Responding…`).
