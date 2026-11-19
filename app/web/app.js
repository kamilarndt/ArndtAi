/* ArndtAi — podgląd sesji (Faza C, runda 19).
 *
 * Klient czyta wyłącznie zamrożony kontrakt `docs/contracts/events.md` (v1):
 * `.agent/sessions/<id>.jsonl`, jedna linia = jeden event.
 *
 * Zasady:
 *  - tylko odczyt: żadnego zapisu, żadnego uruchamiania zadań,
 *  - lista sesji pochodzi z katalogu (whitelist nazw), nigdy ze ścieżki
 *    użytkownika; nazwa z `/` albo `..` jest odrzucana,
 *  - dosyłanie od ostatniego offsetu: czytamy wyłącznie linie zakończone `\n`,
 *    niekompletną końcówkę trzymamy w buforze do następnego ticku (proces
 *    pisze w trakcie),
 *  - brak zapisów: nic nie ląduje na dysku.
 */
"use strict";

const KINDS = [
  "task",
  "thought",
  "tool_call",
  "tool_result",
  "tool_result_cached",
  "tool_result_skipped",
  "denied",
  "store",
  "store_error",
  "error",
  "summary",
  "repair",
];

const KIND_LABEL = {
  task: "task",
  thought: "thought",
  tool_call: "tool_call",
  tool_result: "tool_result",
  tool_result_cached: "tool_result_cached",
  tool_result_skipped: "tool_result_skipped",
  denied: "denied",
  store: "store",
  store_error: "store_error",
  error: "error",
  summary: "summary",
  repair: "repair",
};

const ICON = {
  task: "❯",
  thought: "◆",
  tool_call: "▸",
  tool_result: "✓",
  tool_result_cached: "♻",
  tool_result_skipped: "⊘",
  denied: "⛔",
  store: "◇",
  store_error: "⚠",
  error: "✖",
  summary: "■",
  repair: "↻",
};

const ENDPOINT = "/.agent/sessions/";
const TICK_MS = 2000;

const state = {
  sessions: [], // { id, name, size, mtime, task, status, lastAt }
  selected: null,
  events: [],
  filter: new Set(KINDS),
  query: "",
  skippedLines: 0,
  metaCache: new Map(), // id -> { key, at, meta }
  listingKey: "",
  conn: "wait",
  listingOk: true,
};

const tail = {
  id: null,
  offset: 0, // bajty zużytych, kompletnych linii
  pending: new Uint8Array(0), // bajty niekompletnej końcówki
  loadedSize: 0,
  inFlight: false,
};

/** Rozmiary plików z HEAD — spis katalogu `http.server` ich nie podaje. */
const sizeCache = new Map(); // id -> { size, at }

const $ = (sel) => document.querySelector(sel);

/* ── narzędzia tekstowe ─────────────────────────────────────────────── */

/** Składa tekst do porównań: małe litery, bez diakrytyków, z mapą indeksów. */
function fold(text) {
  let folded = "";
  const map = [];
  for (let i = 0; i < text.length; i++) {
    const chunk = text[i].toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g, "");
    for (const ch of chunk) {
      folded += ch;
      map.push(i);
    }
  }
  return { folded, map };
}

function normalize(text) {
  return fold(text).folded;
}

function startsWithSentence(text, limit) {
  if (!text) return "";
  const cut = text.replace(/\s+/g, " ").trim();
  const sentence = cut.split(/(?<=[.!?])\s/)[0] || cut;
  return sentence.length > limit ? sentence.slice(0, limit - 1) + "…" : sentence;
}

function two(n) {
  return String(n).padStart(2, "0");
}

function clockTime(at) {
  if (!Number.isFinite(at) || at <= 0) return "--:--:--";
  const d = new Date(at * 1000);
  return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
}

function listTime(at) {
  if (!Number.isFinite(at) || at <= 0) return "—";
  const d = new Date(at * 1000);
  const now = new Date();
  const sameDay =
    d.getFullYear() === now.getFullYear() &&
    d.getMonth() === now.getMonth() &&
    d.getDate() === now.getDate();
  if (sameDay) return `${two(d.getHours())}:${two(d.getMinutes())}`;
  return `${two(d.getDate())}.${two(d.getMonth() + 1)} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

function pad(n) {
  return String(n);
}

/* ── whitelist nazw plików ──────────────────────────────────────────── */

/** Zwraca id sesji albo null. Odrzuca `/`, `..`, puste i obce rozszerzenia. */
function safeId(name) {
  if (typeof name !== "string") return null;
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]*\.jsonl$/.test(name)) return null;
  const id = name.slice(0, -".jsonl".length);
  if (!id || id === "." || id === ".." || id.includes("..")) return null;
  return id;
}

function isKnownId(id) {
  return state.sessions.some((s) => s.id === id);
}

/** Adres pliku sesji. Nazwa musi przejść whitelistę — nigdy ze ścieżki użytkownika. */
function sessionUrl(id) {
  if (safeId(id + ".jsonl") !== id) throw new Error("zła nazwa sesji");
  return ENDPOINT + encodeURIComponent(id) + ".jsonl";
}

/* ── parsowanie eventów ─────────────────────────────────────────────── */

function parseLine(line) {
  if (!line.trim()) return null;
  let raw;
  try {
    raw = JSON.parse(line);
  } catch {
    return null;
  }
  if (!raw || typeof raw !== "object") return null;
  if (!KINDS.includes(raw.kind)) return null;
  const data = raw.data && typeof raw.data === "object" ? raw.data : {};
  return { at: Number(raw.at) || 0, kind: raw.kind, data };
}

function summarizeEvents(events) {
  let task = "";
  let status = "brak";
  let lastAt = 0;
  for (const ev of events) {
    if (ev.kind === "task" && !task) {
      task = typeof ev.data.text === "string" ? ev.data.text : "";
    }
    if (ev.kind === "summary" && typeof ev.data.status === "string") {
      status = ev.data.status;
    }
    if (ev.at > lastAt) lastAt = ev.at;
  }
  return { task, status, lastAt };
}

/* ── DOM ────────────────────────────────────────────────────────────── */

function clear(node) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

/** Dopisuje tekst z podświetleniem trafień szukania (bez innerHTML). */
function appendText(parent, text, cls) {
  const value = String(text == null ? "" : text);
  const target = cls ? document.createElement("span") : parent;
  if (cls) target.className = cls;

  const needle = normalize(state.query.trim());
  if (!needle) {
    target.appendChild(document.createTextNode(value));
  } else {
    const { folded, map } = fold(value);
    const hits = [];
    let from = 0;
    for (;;) {
      const idx = folded.indexOf(needle, from);
      if (idx === -1) break;
      const start = map[idx];
      const end = map[idx + needle.length - 1] + 1;
      hits.push([start, end]);
      from = idx + needle.length;
    }
    let cursor = 0;
    for (const [start, end] of hits) {
      if (start < cursor) continue;
      if (start > cursor) target.appendChild(document.createTextNode(value.slice(cursor, start)));
      const mark = document.createElement("mark");
      mark.textContent = value.slice(start, end);
      target.appendChild(mark);
      cursor = end;
    }
    if (cursor < value.length) target.appendChild(document.createTextNode(value.slice(cursor)));
  }

  if (cls) parent.appendChild(target);
  return target;
}

function el(tag, cls, text) {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text != null) node.textContent = text;
  return node;
}

/* ── lista sesji ────────────────────────────────────────────────────── */

async function fetchListing() {
  let res;
  try {
    res = await fetch(ENDPOINT, { cache: "no-store" });
  } catch {
    state.listingOk = false;
    throw new Error("serwer nie odpowiada");
  }
  if (res.status === 404) {
    state.listingOk = false;
    return [];
  }
  if (!res.ok) {
    state.listingOk = false;
    throw new Error("HTTP " + res.status);
  }
  state.listingOk = true;
  const html = await res.text();
  const doc = new DOMParser().parseFromString(html, "text/html");
  const out = [];
  for (const a of doc.querySelectorAll("a[href]")) {
    const href = a.getAttribute("href") || "";
    if (!href.endsWith(".jsonl")) continue;
    let name;
    try {
      name = decodeURIComponent(href);
    } catch {
      continue;
    }
    if (name.includes("/")) continue;
    const id = safeId(name);
    if (!id) continue;
    const row = a.closest("li") || a.parentElement;
    const rowText = row ? row.textContent : "";
    const sizeMatch = rowText.match(/(\d[\d,]*)\s*$/);
    const mtimeMatch = rowText.match(/\d{2}-[A-Za-z]{3}-\d{4} \d{2}:\d{2}/);
    out.push({
      id,
      name,
      size: sizeMatch ? Number(sizeMatch[1].replace(/,/g, "")) : null,
      mtime: mtimeMatch ? mtimeMatch[0] : null,
    });
  }
  return out;
}

/** Rozmiar pliku sesji; HEAD jest tani i pozwala pominąć pobieranie bez zmian. */
async function fileSize(id) {
  const cached = sizeCache.get(id);
  if (cached && Date.now() - cached.at < 1500) return cached.size;
  let size = null;
  try {
    const res = await fetch(sessionUrl(id), { method: "HEAD", cache: "no-store" });
    if (res.ok) {
      const len = Number(res.headers.get("content-length"));
      if (Number.isFinite(len)) size = len;
    }
  } catch {
    size = null;
  }
  sizeCache.set(id, { size, at: Date.now() });
  return size;
}

async function loadMeta(entry) {
  const key = `${entry.size == null ? "?" : entry.size}|${entry.mtime || "?"}`;
  const cached = state.metaCache.get(entry.id);
  const fresh = cached && cached.key === key && key !== "?|?";
  const recent = cached && key === "?|?" && Date.now() - cached.at < 4000;
  if (fresh || recent) return cached.meta;

  let meta = { task: "", status: "brak", lastAt: 0 };
  try {
    const res = await fetch(sessionUrl(entry.id), { cache: "no-store" });
    if (res.ok) {
      const text = await res.text();
      // Ta sama reguła co przy dosyłaniu: liczy się tylko linia zakończona `\n`.
      const complete = text.slice(0, text.lastIndexOf("\n") + 1);
      const events = [];
      for (const line of complete.split("\n")) {
        const ev = parseLine(line);
        if (ev) events.push(ev);
      }
      meta = summarizeEvents(events);
    }
  } catch {
    /* wpis zostaje z poprzedniego odczytu; lista nie może zniknąć */
  }
  state.metaCache.set(entry.id, { key, at: Date.now(), meta });
  return meta;
}

async function reconcileSessions() {
  const listing = await fetchListing();
  const key = listing.map((e) => `${e.name}:${e.size}:${e.mtime}`).join("|");
  const previous = state.sessions;
  const next = [];

  for (const entry of listing) {
    const size = entry.size == null ? await fileSize(entry.id) : entry.size;
    const sized = { ...entry, size };
    const meta = await loadMeta(sized);
    const old = previous.find((s) => s.id === entry.id);
    next.push({
      ...sized,
      task: meta.task,
      status: meta.status,
      lastAt: meta.lastAt || (old ? old.lastAt : 0),
    });
  }

  next.sort((a, b) => b.lastAt - a.lastAt || (a.id < b.id ? 1 : -1));
  state.sessions = next;
  state.listingKey = key;

  if (state.selected && !next.some((s) => s.id === state.selected)) {
    state.selected = null;
    resetTail();
    state.events = [];
  }
  if (!state.selected && next.length) {
    selectSession(next[0].id, false);
  }
  renderSessions();
  renderHeader();
}

function renderSessions() {
  const list = $("#session-list");
  const signature = state.sessions
    .map((s) => `${s.id}|${s.lastAt}|${s.status}|${startsWithSentence(s.task, 160)}|${s.id === state.selected}`)
    .join("§");
  if (signature === list.dataset.signature) return;
  list.dataset.signature = signature;

  clear(list);
  $("#session-count").textContent = String(state.sessions.length);

  for (const s of state.sessions) {
    const li = el("li", "session");
    li.dataset.id = s.id;
    li.tabIndex = 0;
    li.setAttribute("role", "button");
    li.setAttribute("aria-pressed", s.id === state.selected ? "true" : "false");
    if (s.id === state.selected) li.classList.add("active");

    li.appendChild(el("span", "session-id", s.id));
    li.appendChild(el("span", "session-time", listTime(s.lastAt)));
    li.appendChild(el("span", "session-task", startsWithSentence(s.task, 160) || "(bez zadania)"));

    const meta = el("div", "session-meta");
    const badge = el("span", "small-badge", s.status === "brak" ? "brak" : s.status);
    if (s.status === "DONE") badge.classList.add("ok");
    else if (s.status.startsWith("FAILED")) badge.classList.add("fail");
    else if (s.status === "brak") badge.classList.add("none");
    meta.appendChild(badge);
    li.appendChild(meta);

    li.addEventListener("click", () => selectSession(s.id));
    li.addEventListener("keydown", (ev) => {
      if (ev.key === "Enter" || ev.key === " ") {
        ev.preventDefault();
        selectSession(s.id);
      }
    });
    list.appendChild(li);
  }

  $("#sessions-empty").hidden = state.sessions.length !== 0;
}

/* ── wybór sesji i dosyłanie ────────────────────────────────────────── */

function resetTail() {
  tail.id = null;
  tail.offset = 0;
  tail.pending = new Uint8Array(0);
  tail.loadedSize = 0;
}

function selectSession(id, force) {
  if (!isKnownId(id)) return;
  if (!force && state.selected === id) return;
  state.selected = id;
  state.events = [];
  state.skippedLines = 0;
  resetTail();
  tail.id = id;
  renderSessions();
  renderHeader();
  $("#controls").hidden = false;
  $("#events-empty").hidden = true;
  renderEvents(true);
  renderStatus();
  const list = $("#events");
  list.scrollTop = 0;
  tickSelected();
}

function concatBytes(a, b) {
  if (!a.length) return b;
  if (!b.length) return a;
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

async function tickSelected() {
  if (!state.selected || tail.inFlight) return;
  const s = state.sessions.find((x) => x.id === state.selected);
  if (!s) return;
  const knownSize = Number.isFinite(s.size) ? s.size : null;
  if (knownSize !== null && knownSize === tail.loadedSize && tail.loadedSize > 0) {
    return; // plik nie urósł — zero pobierania
  }

  tail.inFlight = true;
  try {
    let start = tail.offset + tail.pending.length;
    const res = await fetch(sessionUrl(state.selected), {
      cache: "no-store",
      headers: start > 0 ? { Range: `bytes=${start}-` } : {},
    });

    if (res.status === 404) {
      setConn("err", "sesja zniknęła z dysku");
      return;
    }
    if (res.status === 416) {
      // Plik skrócił się albo został podmieniony — czytamy od nowa.
      state.events = [];
      state.skippedLines = 0;
      resetTail();
      tail.id = state.selected;
      start = 0;
      setConn("ok", "plik zmieniony — czytam od nowa");
      schedule();
      return;
    }
    if (!res.ok && res.status !== 206) {
      setConn("err", `HTTP ${res.status} przy odczycie sesji`);
      return;
    }

    const body = new Uint8Array(await res.arrayBuffer());
    let chunk;
    if (res.status === 206) {
      chunk = body;
    } else {
      if (body.length < tail.offset) {
        // Plik podmieniony na krótszy — zaczynamy od nowa, bez śmieci.
        state.events = [];
        state.skippedLines = 0;
        resetTail();
        tail.id = state.selected;
        start = 0;
      }
      chunk = body.subarray(Math.min(start, body.length));
      tail.loadedSize = body.length;
    }

    const data = concatBytes(tail.pending, chunk);
    const lastNewline = data.lastIndexOf(10);
    if (lastNewline === -1) {
      tail.pending = data;
      return;
    }

    const complete = data.subarray(0, lastNewline + 1);
    tail.pending = data.subarray(lastNewline + 1);

    const text = new TextDecoder("utf-8").decode(complete);
    let added = 0;
    for (const line of text.split("\n")) {
      if (!line.trim()) continue;
      const ev = parseLine(line);
      if (!ev) {
        state.skippedLines += 1;
        continue;
      }
      state.events.push(ev);
      added += 1;
    }

    tail.offset += complete.length;
    tail.loadedSize = tail.offset + tail.pending.length;

    const meta = summarizeEvents(state.events);
    const row = state.sessions.find((x) => x.id === state.selected);
    if (row) {
      row.task = meta.task;
      row.status = meta.status;
      row.lastAt = meta.lastAt || row.lastAt;
      row.size = tail.loadedSize;
      state.metaCache.set(row.id, {
        key: `${row.size}|${row.mtime || ""}`,
        at: Date.now(),
        meta,
      });
    }

    setConn("ok", "odczyt działa");
    if (added || state.events.length === 0) {
      renderEvents();
      renderStatus();
    }
  } catch {
    setConn("err", "nie mogę odczytać pliku sesji");
  } finally {
    tail.inFlight = false;
  }
}

/* ── nagłówek i pasek stanu ─────────────────────────────────────────── */

function lastSummary() {
  for (let i = state.events.length - 1; i >= 0; i--) {
    if (state.events[i].kind === "summary") return state.events[i];
  }
  return null;
}

function counters() {
  const summary = lastSummary();
  const d = summary ? summary.data : {};
  let steps = 0;
  let tools = 0;
  let denied = 0;
  for (const ev of state.events) {
    if (Number.isFinite(ev.data.step)) steps = Math.max(steps, ev.data.step);
    if (ev.kind === "tool_call") tools += 1;
    if (ev.kind === "denied") denied += 1;
  }
  return {
    status: summary && d.status ? d.status : state.events.length ? "w toku" : "brak",
    steps: Number.isFinite(d.steps) ? d.steps : steps,
    toolCalls: Number.isFinite(d.tool_calls) ? d.tool_calls : tools,
    denied: Number.isFinite(d.denied) ? d.denied : denied,
    drift: Number.isFinite(d.drift) ? d.drift : null,
  };
}

function statusClass(status) {
  if (status === "DONE") return "ok";
  if (status.startsWith("FAILED")) return "fail";
  if (status === "w toku") return "warn";
  return "none";
}

function renderHeader() {
  const s = state.sessions.find((x) => x.id === state.selected);
  const title = $("#session-title");
  const sub = $("#session-sub");
  const badge = $("#session-status");

  if (!s) {
    title.textContent = "Wybierz sesję";
    sub.textContent = state.listingOk
      ? state.sessions.length
        ? ""
        : "katalog .agent/sessions jest pusty"
      : "brak katalogu .agent/sessions — serwuj stronę z korzenia projektu";
    badge.textContent = "—";
    badge.className = "badge none";
    return;
  }

  title.textContent = s.id;
  sub.textContent = `.agent/sessions/${s.id}.jsonl · ${state.events.length} eventów · ostatni ${clockTime(
    s.lastAt
  )}`;

  const c = counters();
  badge.textContent = c.status === "brak" ? "brak" : c.status;
  badge.className = "badge " + statusClass(c.status);
}

function renderStatus() {
  const c = counters();
  const box = $("#status-counters");
  if (!state.selected) {
    box.textContent = "—";
    return;
  }
  box.textContent = "";
  const parts = [
    ["kroki", c.steps],
    ["narzędzia", c.toolCalls],
    ["odmowy", c.denied],
  ];
  for (const [label, value] of parts) {
    const span = el("span", null, `${label} `);
    span.appendChild(el("b", null, pad(value)));
    box.appendChild(span);
  }
  if (c.drift != null) {
    const span = el("span", null, "dryf ");
    span.appendChild(el("b", null, pad(c.drift)));
    box.appendChild(span);
  }
  const status = el("span", "status-" + statusClass(c.status), c.status);
  box.appendChild(status);
  if (state.skippedLines) {
    box.appendChild(el("span", null, `· pominięto ${state.skippedLines} niepełnych linii`));
  }
}

function setConn(kind, label) {
  state.conn = kind;
  const dot = $("#conn-dot");
  dot.className = "pulse " + (kind === "ok" ? "ok" : kind === "err" ? "err" : "");
  $("#conn-label").textContent = label;
}

/* ── filtr typów ────────────────────────────────────────────────────── */

function buildKindFilters() {
  const box = $("#kinds");
  clear(box);
  for (const kind of KINDS) {
    const label = el("label", "kind checked");
    label.dataset.kind = kind;
    label.classList.add("k-" + kind);

    const input = document.createElement("input");
    input.type = "checkbox";
    input.checked = true;
    input.dataset.kind = kind;
    input.setAttribute("aria-label", `Pokaż ${kind}`);
    input.addEventListener("change", () => {
      if (input.checked) state.filter.add(kind);
      else state.filter.delete(kind);
      label.classList.toggle("checked", input.checked);
      renderEvents();
    });

    label.appendChild(input);
    label.appendChild(el("span", "kind-name", KIND_LABEL[kind]));
    const count = el("span", "n", "0");
    count.dataset.count = kind;
    label.appendChild(count);
    box.appendChild(label);
  }
}

function syncKindCounts() {
  const counts = Object.fromEntries(KINDS.map((k) => [k, 0]));
  for (const ev of state.events) counts[ev.kind] = (counts[ev.kind] || 0) + 1;
  for (const kind of KINDS) {
    const node = document.querySelector(`[data-count="${kind}"]`);
    if (node) node.textContent = String(counts[kind] || 0);
  }
}

/* ── wiersze eventów ────────────────────────────────────────────────── */

function compact(value, limit) {
  if (value == null) return "";
  if (typeof value === "string") return value.length > limit ? value.slice(0, limit - 1) + "…" : value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  let text;
  try {
    text = JSON.stringify(value);
  } catch {
    text = String(value);
  }
  return text.length > limit ? text.slice(0, limit - 1) + "…" : text;
}

function pretty(value) {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function addArgs(body, args) {
  if (!args || typeof args !== "object") return;
  const keys = Object.keys(args);
  if (!keys.length) return;
  const inline = keys.map((k) => `${k}=${compact(args[k], 90)}`).join("  ");
  const pre = el("pre", "ev-args");
  appendText(pre, inline);
  body.appendChild(pre);

  const long = keys.some((k) => typeof args[k] === "string" && args[k].length > 90);
  if (long) {
    const details = document.createElement("details");
    const summary = document.createElement("summary");
    summary.textContent = "pełne argumenty";
    details.appendChild(summary);
    const full = el("pre", "ev-args");
    appendText(full, pretty(args));
    details.appendChild(full);
    body.appendChild(details);
  }
}

function addOutput(body, output, open) {
  if (output == null) return;
  const details = document.createElement("details");
  if (open) details.open = true;
  const summary = document.createElement("summary");
  summary.textContent = typeof output === "object" ? "wynik" : "treść";
  details.appendChild(summary);
  const pre = el("pre", "ev-args");
  if (typeof output === "string") appendText(pre, output);
  else if (output && typeof output === "object" && typeof output.content === "string") {
    appendText(pre, output.content);
    const rest = { ...output };
    delete rest.content;
    if (Object.keys(rest).length) {
      pre.appendChild(document.createTextNode("\n\n"));
      appendText(pre, pretty(rest));
    }
  } else appendText(pre, pretty(output));
  details.appendChild(pre);
  body.appendChild(details);
}

function head(body, label, detail, extra) {
  const line = el("div", "ev-head");
  if (label) {
    const name = el("span", "ev-label");
    appendText(name, label);
    line.appendChild(name);
  }
  if (detail != null) appendText(line, detail, "ev-detail");
  if (extra) line.appendChild(extra);
  body.appendChild(line);
  return line;
}

function buildRow(ev, idx) {
  const li = el("li", "ev ev-" + ev.kind);
  if (ev.kind === "tool_result" && ev.data.ok === false) li.classList.add("fail");

  li.appendChild(el("span", "ev-time", clockTime(ev.at)));
  li.appendChild(el("span", "ev-icon", ICON[ev.kind] || "•"));
  const body = el("div", "ev-body");
  li.appendChild(body);
  li.dataset.idx = String(idx);
  li.dataset.kind = ev.kind;

  const d = ev.data || {};

  switch (ev.kind) {
    case "task": {
      head(body, "zadanie");
      const text = el("p", "ev-text");
      appendText(text, d.text || "(brak treści)");
      body.appendChild(text);
      break;
    }
    case "thought": {
      head(body, "thought", Number.isFinite(d.step) ? `krok ${d.step}` : null);
      const text = el("p", "ev-text thought");
      appendText(text, d.thought || "");
      body.appendChild(text);
      break;
    }
    case "tool_call": {
      const line = el("div", "ev-head");
      const name = el("span", "ev-label");
      appendText(name, "tool_call");
      line.appendChild(name);
      line.appendChild(el("span", "ev-tool", d.tool || "?"));
      if (Number.isFinite(d.step)) line.appendChild(el("span", "ev-meta", `krok ${d.step}`));
      if (d.args && typeof d.args === "object" && typeof d.args.path === "string") {
        appendText(line, d.args.path, "ev-path");
      }
      body.appendChild(line);
      addArgs(body, d.args);
      break;
    }
    case "tool_result": {
      const ok = d.ok !== false;
      const extra = el("span", "small-badge " + (ok ? "ok" : "fail"), ok ? "ok" : "błąd");
      head(body, "tool_result", null, extra);
      const line = body.querySelector(".ev-head");
      line.appendChild(el("span", "ev-tool", d.tool || "?"));
      if (d.code) line.appendChild(el("span", "ev-meta", String(d.code)));
      if (Number.isFinite(d.bytes)) line.appendChild(el("span", "ev-meta", `${d.bytes} B`));
      addOutput(body, d.output, d.tool === "fs.read" && ok);
      break;
    }
    case "tool_result_cached": {
      head(body, "tool_result_cached", "z zapisanego kroku");
      const line = body.querySelector(".ev-head");
      line.appendChild(el("span", "ev-tool", d.tool || "?"));
      if (Number.isFinite(d.step)) line.appendChild(el("span", "ev-meta", `krok ${d.step}`));
      addOutput(body, d.result, false);
      break;
    }
    case "tool_result_skipped": {
      head(body, "tool_result_skipped", "at-most-once");
      const line = body.querySelector(".ev-head");
      line.appendChild(el("span", "ev-tool", d.tool || "?"));
      if (d.reason) line.appendChild(el("span", "ev-meta", String(d.reason)));
      break;
    }
    case "denied": {
      const extra = el("span", "small-badge warn", "bramka");
      head(body, "denied", null, extra);
      const line = body.querySelector(".ev-head");
      line.appendChild(el("span", "ev-tool", d.tool || "?"));
      if (Number.isFinite(d.step)) line.appendChild(el("span", "ev-meta", `krok ${d.step}`));
      if (d.drift) line.appendChild(el("span", "ev-meta", "dryf"));
      const text = el("p", "ev-text");
      appendText(text, d.reason || "");
      body.appendChild(text);
      break;
    }
    case "store": {
      head(body, "store");
      const line = body.querySelector(".ev-head");
      line.appendChild(el("span", "ev-path", d.path || "?"));
      const bits = [];
      if (d.journal_mode) bits.push(`journal ${d.journal_mode}`);
      if (Number.isFinite(d.busy_timeout)) bits.push(`busy_timeout ${d.busy_timeout} ms`);
      if (bits.length) line.appendChild(el("span", "ev-meta", "· " + bits.join(" · ")));
      break;
    }
    case "store_error": {
      head(body, "store_error", d.stage ? String(d.stage) : null);
      const text = el("p", "ev-text");
      appendText(text, d.message || "");
      body.appendChild(text);
      break;
    }
    case "error": {
      head(body, "error");
      const text = el("p", "ev-text");
      appendText(text, d.message || "");
      body.appendChild(text);
      break;
    }
    case "summary": {
      const status = d.status || "brak";
      const line = el("div", "ev-head");
      const name = el("span", "ev-label");
      appendText(name, "summary");
      line.appendChild(name);
      const badge = el("span", "summary-status " + statusClass(status), status);
      line.appendChild(badge);
      body.appendChild(line);

      const grid = el("div", "counters");
      for (const [label, value] of [
        ["steps", d.steps],
        ["tool_calls", d.tool_calls],
        ["denied", d.denied],
        ["drift", d.drift],
      ]) {
        const item = el("span");
        appendText(item, `${label} `);
        item.appendChild(el("b", null, Number.isFinite(value) ? String(value) : "—"));
        grid.appendChild(item);
      }
      if (d.reason) {
        const reason = el("span", null, "· ");
        appendText(reason, String(d.reason));
        grid.appendChild(reason);
      }
      body.appendChild(grid);
      break;
    }
    case "repair": {
      head(body, "repair", Number.isFinite(d.step) ? `krok ${d.step}` : null);
      const text = el("p", "ev-text");
      appendText(text, d.detail || "");
      body.appendChild(text);
      break;
    }
    default: {
      head(body, ev.kind);
      const text = el("p", "ev-text");
      appendText(text, compact(d, 400));
      body.appendChild(text);
    }
  }

  return li;
}

function renderEvents(scrollToBottom) {
  const list = $("#events");
  const wasAtBottom = list.scrollHeight - list.scrollTop - list.clientHeight < 80;
  const keep = list.scrollTop;

  const needle = normalize(state.query.trim());
  clear(list);
  let visible = 0;

  state.events.forEach((ev, idx) => {
    const row = buildRow(ev, idx);
    const inKind = state.filter.has(ev.kind);
    const textHit = !needle || normalize(row.textContent).includes(needle);
    if (!inKind || !textHit) row.hidden = true;
    else visible += 1;
    list.appendChild(row);
  });

  syncKindCounts();
  $("#visible-count").textContent = state.events.length
    ? `widoczne ${visible} z ${state.events.length}`
    : "";

  const empty = $("#events-empty");
  if (!state.selected) {
    empty.hidden = false;
    empty.firstElementChild.textContent = "Wybierz sesję z listy po lewej.";
  } else if (!state.events.length) {
    empty.hidden = false;
    empty.firstElementChild.textContent = "Sesja jest pusta — czekam na pierwszy event.";
  } else if (!visible) {
    empty.hidden = false;
    empty.firstElementChild.textContent = "Nic nie pasuje do filtra ani szukania.";
  } else {
    empty.hidden = true;
  }

  if (scrollToBottom || wasAtBottom) list.scrollTop = list.scrollHeight;
  else list.scrollTop = keep;

  renderHeader();
  renderStatus();
}

/* ── pętla odświeżania ──────────────────────────────────────────────── */

let timer = null;

function schedule() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(tick, TICK_MS);
}

async function tick() {
  try {
    await reconcileSessions();
    await tickSelected();
    if (state.conn !== "err") setConn("ok", "odczyt działa");
  } catch {
    setConn("err", "serwer nie odpowiada — czy działa python3 -m http.server?");
  }
  schedule();
}

/* ── start ──────────────────────────────────────────────────────────── */

function startClock() {
  const paint = () => {
    const d = new Date();
    $("#clock").textContent = `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
  };
  paint();
  setInterval(paint, 1000);
}

function bindControls() {
  const search = $("#search");
  search.addEventListener("input", () => {
    state.query = search.value;
    $("#search-clear").hidden = !search.value;
    renderEvents();
  });
  $("#search-clear").addEventListener("click", () => {
    search.value = "";
    state.query = "";
    $("#search-clear").hidden = true;
    renderEvents();
    search.focus();
  });
  $("#kinds-all").addEventListener("click", () => {
    state.filter = new Set(KINDS);
    for (const input of document.querySelectorAll('#kinds input[type="checkbox"]')) {
      input.checked = true;
      input.closest(".kind").classList.add("checked");
    }
    renderEvents();
  });
  $("#kinds-none").addEventListener("click", () => {
    state.filter.clear();
    for (const input of document.querySelectorAll('#kinds input[type="checkbox"]')) {
      input.checked = false;
      input.closest(".kind").classList.remove("checked");
    }
    renderEvents();
  });
  document.addEventListener("keydown", (ev) => {
    if (ev.key === "/" && document.activeElement !== search && !ev.metaKey && !ev.ctrlKey) {
      ev.preventDefault();
      search.focus();
    } else if (ev.key === "Escape" && document.activeElement === search) {
      search.value = "";
      state.query = "";
      $("#search-clear").hidden = true;
      renderEvents();
    }
  });
}

buildKindFilters();
bindControls();
startClock();
renderHeader();
renderStatus();
setConn("wait", "łączę…");
tick();
