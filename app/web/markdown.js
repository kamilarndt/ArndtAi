/* ArndtAi — markdown w treści asystenta (runda 20).
 *
 * Odpowiednik `app/src/markdown.rs` (runda 18) po stronie web UI:
 * ten sam zakres, ta sama kolejność rozpoznawania bloków, ta sama paleta.
 *
 *   # nagłówek      → pogrubiony tekst bez hashy (h1-h3 jak w TUI, h4+ przygaszony)
 *   **pogrubienie** → atrybut, *kursywa* / _kursywa_ → atrybut
 *   `kod`           → inne tło (BG_LIGHT jak w TUI)
 *   - punkt / 1.    → wcięcie + punktor, numeracja zachowana
 *   ```lang         → ramka + etykieta języka + podświetlanie rust/json/bash/toml
 *   > cytat         → pionowa kreska, przygaszony tekst
 *   | a | b |       → siatka, kolumny bez zawijania
 *   ---             → reguła `——` (jak `markdown::RULE_TEXT`)
 *
 * Świadomie nie renderujemy: obrazów (podpis + adres w nawiasie), klikalnych
 * linków (podkreślony tekst + adres w nawiasie), HTML, LaTeX-a, mermaid.
 * Emoji zostają bez zmian.
 *
 * Niepełny markdown nie może wywalić renderu: brak zamknięcia `**`, `*`, `` ` ``
 * albo ``` zostawia znaki tak, jak przyszły (blok kodu bez zamknięcia pokazuje
 * resztę tekstu w ramce — tak samo robi TUI).
 *
 * Plik nie zna pojęcia eventu: dostaje tekst i funkcję `text(...)`, która
 * zamienia kawałek tekstu na węzeł DOM. Dzięki temu app.js dokłada do środka
 * podświetlenie trafień szukania, a nie surowe `textContent`.
 */
"use strict";

(function (global) {
  /** Powyżej tylu linii blok kodu jest obcinany z `… (N więcej)` (jak TUI). */
  const CODE_MAX_LINES = 200;
  /** Linia kodu dłuższa niż tyle znaków jest ucinana z `… +N znaków`. */
  const CODE_MAX_COLS = 120;
  const BULLET = "\u2022";
  const RULE_TEXT = "\u2014\u2014";
  const QUOTE_BAR = "\u2502";
  const MAX_DEPTH = 4;

  /* ── drobne pomocniki DOM ──────────────────────────────────────────── */

  function el(tag, cls, text) {
    const node = document.createElement(tag);
    if (cls) node.className = cls;
    if (text != null) node.textContent = text;
    return node;
  }

  /** Węzeł z tekstem: podświetlenie trafień, gdy app.js je poda. */
  function textNode(opts, value) {
    if (opts && typeof opts.text === "function") return opts.text(value);
    return document.createTextNode(value);
  }

  function span(cls, value, opts) {
    const node = el("span", cls);
    node.appendChild(textNode(opts, value));
    return node;
  }

  function widthOf(text) {
    return Array.from(text).length;
  }

  /* ── inline ────────────────────────────────────────────────────────── */

  /**
   * Rozbija linię na odcinki `{ text, cls }`. `base` to klasa tekstu
   * otaczającego (np. `md-b` w nagłówku). Niepełne znaczniki zostają tekstem.
   */
  function inlineSegments(text, base, depth) {
    const out = [];
    let plain = "";
    let i = 0;
    const flush = () => {
      if (plain) {
        out.push({ text: plain, cls: base });
        plain = "";
      }
    };
    if (depth > MAX_DEPTH) return [{ text, cls: base }];

    while (i < text.length) {
      const rest = text.slice(i);

      // `` `kod` `` — sprawdzany pierwszy, żeby `**` w środku został tekstem.
      if (rest[0] === "`") {
        const close = rest.indexOf("`", 1);
        if (close > 1) {
          flush();
          out.push({ text: rest.slice(1, close), cls: "md-c" });
          i += close + 1;
          continue;
        }
      }
      // `![alt](url)` — obraz: sam podpis i adres w nawiasie, bez obrazka.
      if (rest.startsWith("![")) {
        const parts = linkParts(rest, 2);
        if (parts) {
          flush();
          out.push({ text: parts.label, cls: base });
          out.push({ text: ` (${parts.url})`, cls: "md-dim" });
          i += parts.consumed;
          continue;
        }
      }
      // `[tekst](url)` — tekst podkreślony + adres w nawiasie, bez klikania.
      if (rest[0] === "[") {
        const parts = linkParts(rest, 1);
        if (parts) {
          flush();
          out.push({ text: parts.label, cls: joinCls(base, "md-link") });
          out.push({ text: ` (${parts.url})`, cls: "md-dim" });
          i += parts.consumed;
          continue;
        }
      }
      if (rest.startsWith("**")) {
        const close = rest.indexOf("**", 2);
        if (close > 2) {
          flush();
          out.push(...inlineSegments(rest.slice(2, close), "md-b", depth + 1));
          i += close + 2;
          continue;
        }
      }
      if (rest[0] === "*" && !rest.startsWith("**")) {
        const close = rest.indexOf("*", 1);
        if (close > 1 && !rest.startsWith("*", close + 1)) {
          flush();
          out.push(...inlineSegments(rest.slice(1, close), "md-i", depth + 1));
          i += close + 1;
          continue;
        }
      }
      if (rest[0] === "_") {
        const close = rest.indexOf("_", 1);
        if (close > 1) {
          const inner = rest.slice(1, close);
          const after = rest[close + 1];
          const boundary = after === undefined || !/[\p{L}\p{N}]/u.test(after);
          if (!inner.startsWith(" ") && !inner.endsWith(" ") && boundary) {
            flush();
            out.push(...inlineSegments(inner, "md-i", depth + 1));
            i += close + 1;
            continue;
          }
        }
      }

      plain += rest[0];
      i += 1;
    }
    flush();
    return out;
  }

  function joinCls(base, extra) {
    return base ? `${base} ${extra}` : extra;
  }

  /**
   * `[tekst](adres)` → { label, url, consumed }. `open` to 1 dla linku
   * i 2 dla obrazu (`![`). Bez zamknięcia `](` albo `)` zwraca null.
   */
  function linkParts(text, open) {
    const close = text.indexOf("](", open);
    if (close === -1) return null;
    const label = text.slice(open, close);
    const after = text.slice(close + 2);
    const end = after.indexOf(")");
    if (end === -1) return null;
    const url = after.slice(0, end).trim();
    if (!label && !url) return null;
    return { label, url, consumed: close + 2 + end + 1 };
  }

  /** Wstawia odcinki inline do `parent` (spany z klasą albo czysty tekst). */
  function appendInline(parent, text, base, opts) {
    for (const seg of inlineSegments(text, base, 0)) {
      if (!seg.text) continue;
      if (seg.cls) parent.appendChild(span(seg.cls, seg.text, opts));
      else parent.appendChild(textNode(opts, seg.text));
    }
    return parent;
  }

  /* ── rozpoznawanie bloków ──────────────────────────────────────────── */

  function fenceStart(line) {
    const trimmed = line.trimStart();
    for (const fence of ["```", "~~~"]) {
      if (trimmed.startsWith(fence)) {
        const info = trimmed.slice(fence.length);
        const lang = info.split(/\s+/).filter(Boolean)[0] || "";
        return { token: fence, info: lang };
      }
    }
    return null;
  }

  function isFenceEnd(line, token) {
    return line.trimStart().startsWith(token);
  }

  function heading(trimmed) {
    let hashes = 0;
    while (trimmed[hashes] === "#") hashes += 1;
    if (!hashes || hashes > 6) return null;
    const rest = trimmed.slice(hashes);
    if (!rest.startsWith(" ")) return null;
    return { level: hashes, text: rest.slice(1).trimEnd() };
  }

  function isRule(trimmed) {
    const compact = trimmed.replace(/\s+/g, "");
    if (compact.length < 3) return false;
    return /^-+$/.test(compact) || /^\*+$/.test(compact) || /^_+$/.test(compact);
  }

  /** `- punkt`, `* punkt`, `+ punkt`, `1. punkt`, `1) punkt` z wcięciem. */
  function listItem(line) {
    const indent = line.length - line.trimStart().length;
    const trimmed = line.trimStart();
    const depth = Math.min(Math.floor(indent / 2), MAX_DEPTH);
    for (const prefix of ["- ", "* ", "+ "]) {
      if (trimmed.startsWith(prefix)) {
        return { depth, marker: `${BULLET} `, text: trimmed.slice(prefix.length).trimEnd() };
      }
    }
    const digits = trimmed.match(/^\d{1,3}/);
    if (digits) {
      const rest = trimmed.slice(digits[0].length);
      for (const sep of [". ", ") "]) {
        if (rest.startsWith(sep)) {
          return {
            depth,
            marker: `${digits[0]}${sep.trimEnd()} `,
            text: rest.slice(sep.length).trimEnd(),
          };
        }
      }
    }
    return null;
  }

  /** `> cytat`, `>> głębiej` → { depth, rest }. */
  function quoteLine(line) {
    const trimmed = line.trimStart();
    if (!trimmed.startsWith(">")) return null;
    let rest = trimmed;
    let depth = 0;
    while (rest.startsWith(">")) {
      depth += 1;
      rest = rest.slice(1);
      if (rest.startsWith(" ")) rest = rest.slice(1);
    }
    return { depth: Math.min(depth, 3), rest: rest.trimEnd() };
  }

  function isTableRow(line) {
    const trimmed = line.trim();
    return trimmed.startsWith("|") && (trimmed.split("|").length - 1) >= 2;
  }

  function isTableRule(line) {
    const trimmed = line.trim();
    if (!isTableRow(trimmed)) return false;
    const inner = trimmed.replace(/^\|/, "").replace(/\|$/, "");
    if (!inner.trim()) return false;
    return inner.split("|").every((cell) => {
      const value = cell.trim();
      return value.length > 0 && /^[-:]+$/.test(value);
    });
  }

  function tableCells(line) {
    let inner = line.trim();
    if (inner.startsWith("|")) inner = inner.slice(1);
    if (inner.endsWith("|")) inner = inner.slice(0, -1);
    return inner.split("|").map((cell) => cell.trim());
  }

  /* ── bloki: lista, cytat, reguła ───────────────────────────────────── */

  function listRow(item, opts) {
    const row = el("div", "md-li");
    row.style.paddingLeft = `${item.depth * 2}ch`;
    row.appendChild(span("md-marker", item.marker, opts));
    appendInline(row, item.text, "", opts);
    return row;
  }

  function quoteBlock(lines, depth, opts) {
    let core = el("div", "md-q-body");
    for (const line of lines) {
      const row = el("div", "md-q-line");
      appendInline(row, line, "md-dim", opts);
      core.appendChild(row);
    }
    // Każdy poziom cytatu dokłada własną pionową kreskę — jak `│ │ ` w TUI.
    for (let level = 0; level < Math.max(1, depth); level += 1) {
      const box = el("div", "md-quote");
      box.appendChild(core);
      core = box;
    }
    return core;
  }

  function ruleRow() {
    return el("div", "md-rule", RULE_TEXT);
  }

  /* ── blok kodu ─────────────────────────────────────────────────────── */

  const KEYWORDS = {
    rust: [
      "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
      "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod",
      "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super",
      "trait", "true", "type", "unsafe", "use", "where", "while", "union", "yield",
    ],
    json: ["true", "false", "null"],
    bash: [
      "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case",
      "esac", "in", "function", "local", "export", "return", "exit", "set", "unset",
      "readonly", "declare", "shift", "trap", "source", "alias", "echo", "cd", "printf",
      "true", "false",
    ],
    toml: ["true", "false"],
  };

  function langKind(info) {
    const key = (info || "").trim().toLowerCase();
    if (key === "rust" || key === "rs") return "rust";
    if (key === "json" || key === "jsonc") return "json";
    if (key === "bash" || key === "sh" || key === "shell" || key === "zsh" || key === "console") return "bash";
    if (key === "toml") return "toml";
    return "";
  }

  const IDENT_START = /[A-Za-z_]/;
  const IDENT_CHAR = /[A-Za-z0-9_]/;

  function scanIdent(text, start) {
    let i = start;
    while (i < text.length && IDENT_CHAR.test(text[i])) i += 1;
    return i;
  }

  function scanQuoted(text, start, quote) {
    let i = start + 1;
    while (i < text.length) {
      const c = text[i];
      if (quote === '"' && c === "\\") {
        i += 2;
        continue;
      }
      if (c === quote) return i + 1;
      i += 1;
    }
    return text.length;
  }

  /** `'x'`, `'\n'` — tylko literał znaku; `'a` (lifetime) zostaje bez koloru. */
  function scanChar(text, start) {
    if (text[start + 1] === "\\") {
      let i = start + 3;
      while (i < text.length && i <= start + 5) {
        if (text[i] === "'") return i + 1;
        i += 1;
      }
      return -1;
    }
    if (text[start + 2] === "'" && text[start + 1] !== "'") return start + 3;
    return -1;
  }

  function scanNumber(text, start) {
    let i = start;
    if (text[i] === "-") i += 1;
    while (i < text.length && /[A-Za-z0-9_.]/.test(text[i])) i += 1;
    return Math.max(i, start + 1);
  }

  /** Podświetlanie jednej linii: komentarze, stringi, liczby, słowa kluczowe. */
  function highlight(info, line) {
    const kind = langKind(info);
    if (!kind || line === "") return [{ text: line, cls: "" }];
    const words = KEYWORDS[kind] || [];
    const marks = [];
    let i = 0;
    while (i < line.length) {
      const c = line[i];
      if (kind === "json") {
        if (c === '"') {
          const end = scanQuoted(line, i, '"');
          marks.push({ start: i, end, cls: "md-str" });
          i = end;
          continue;
        }
        if (/[0-9]/.test(c) || (c === "-" && /[0-9]/.test(line[i + 1] || ""))) {
          const end = scanNumber(line, i);
          marks.push({ start: i, end, cls: "md-num" });
          i = end;
          continue;
        }
        if (IDENT_START.test(c)) {
          const end = scanIdent(line, i);
          if (words.includes(line.slice(i, end))) marks.push({ start: i, end, cls: "md-kw" });
          i = end;
          continue;
        }
      } else if (kind === "toml") {
        if (c === "#") {
          marks.push({ start: i, end: line.length, cls: "md-com" });
          break;
        }
        if (c === '"' || c === "'") {
          const end = scanQuoted(line, i, c);
          marks.push({ start: i, end, cls: "md-str" });
          i = end;
          continue;
        }
        if (c === "[") {
          const close = line.indexOf("]", i);
          if (close !== -1) {
            marks.push({ start: i, end: close + 1, cls: "md-kw" });
            i = close + 1;
            continue;
          }
        }
        if (/[0-9]/.test(c)) {
          const end = scanNumber(line, i);
          marks.push({ start: i, end, cls: "md-num" });
          i = end;
          continue;
        }
        if (IDENT_START.test(c)) {
          const end = scanIdent(line, i);
          if (words.includes(line.slice(i, end))) marks.push({ start: i, end, cls: "md-kw" });
          i = end;
          continue;
        }
      } else if (kind === "bash") {
        if (c === "#" && (i === 0 || line[i - 1] === " " || line[i - 1] === "\t")) {
          marks.push({ start: i, end: line.length, cls: "md-com" });
          break;
        }
        if (c === '"' || c === "'") {
          const end = scanQuoted(line, i, c);
          marks.push({ start: i, end, cls: "md-str" });
          i = end;
          continue;
        }
        if (/[0-9]/.test(c)) {
          const end = scanNumber(line, i);
          marks.push({ start: i, end, cls: "md-num" });
          i = end;
          continue;
        }
        if (IDENT_START.test(c)) {
          const end = scanIdent(line, i);
          if (words.includes(line.slice(i, end))) marks.push({ start: i, end, cls: "md-kw" });
          i = end;
          continue;
        }
      } else if (kind === "rust") {
        if (c === "/" && line[i + 1] === "/") {
          marks.push({ start: i, end: line.length, cls: "md-com" });
          break;
        }
        if (c === '"') {
          const end = scanQuoted(line, i, '"');
          marks.push({ start: i, end, cls: "md-str" });
          i = end;
          continue;
        }
        if (c === "'") {
          const end = scanChar(line, i);
          if (end > 0) {
            marks.push({ start: i, end: Math.min(end, line.length), cls: "md-str" });
            i = end;
          } else {
            i += 1;
          }
          continue;
        }
        if (/[0-9]/.test(c)) {
          const end = scanNumber(line, i);
          marks.push({ start: i, end, cls: "md-num" });
          i = end;
          continue;
        }
        if (IDENT_START.test(c)) {
          const end = scanIdent(line, i);
          if (words.includes(line.slice(i, end))) marks.push({ start: i, end, cls: "md-kw" });
          i = end;
          continue;
        }
      }
      i += 1;
    }
    return spansFrom(line, marks);
  }

  function spansFrom(line, marks) {
    const out = [];
    let at = 0;
    for (const mark of marks) {
      const start = mark.start;
      const end = Math.min(mark.end, line.length);
      if (start >= end || start < at) continue;
      if (start > at) out.push({ text: line.slice(at, start), cls: "" });
      out.push({ text: line.slice(start, end), cls: mark.cls });
      at = end;
    }
    if (at < line.length) out.push({ text: line.slice(at), cls: "" });
    if (!out.length) out.push({ text: "", cls: "" });
    return out;
  }

  /** Linia szersza niż `max` ucinana z `… +N znaków` (liczy się całość). */
  function clipCodeLine(text, max) {
    const chars = Array.from(text);
    if (chars.length <= max) return { visible: text, suffix: "" };
    for (let keep = max; keep >= 0; keep -= 1) {
      const hidden = chars.length - keep;
      const suffix = ` \u2026 +${hidden} znak\u00f3w`;
      if (keep + widthOf(suffix) <= max) {
        return { visible: chars.slice(0, keep).join(""), suffix };
      }
    }
    return { visible: chars.slice(0, Math.max(0, max - 1)).join("") + "\u2026", suffix: "" };
  }

  function codeBlock(lines, info, opts) {
    const box = el("div", "md-code");
    const lang = (info || "").trim();
    if (lang) {
      box.dataset.lang = lang;
      box.appendChild(el("span", "md-code-label", lang));
    }
    const body = el("div", "md-code-body");
    const total = lines.length;
    const shown = Math.min(total, CODE_MAX_LINES);
    for (let n = 0; n < shown; n += 1) {
      const row = el("div", "md-code-line");
      const { visible, suffix } = clipCodeLine(lines[n], CODE_MAX_COLS);
      const pieces = highlight(lang, visible);
      if (!pieces.length || (pieces.length === 1 && !pieces[0].text)) {
        row.appendChild(document.createTextNode(" "));
      } else {
        for (const piece of pieces) {
          if (!piece.text) continue;
          row.appendChild(piece.cls ? span(piece.cls, piece.text, opts) : textNode(opts, piece.text));
        }
      }
      if (suffix) row.appendChild(span("md-com", suffix, opts));
      body.appendChild(row);
    }
    if (!total) {
      const row = el("div", "md-code-line");
      row.appendChild(document.createTextNode(" "));
      body.appendChild(row);
    }
    if (total > shown) {
      body.appendChild(el("div", "md-code-more", `\u2026 (${total - shown} wi\u0119cej)`));
    }
    box.appendChild(body);
    return box;
  }

  /* ── tabela ────────────────────────────────────────────────────────── */

  function tableBlock(lines, opts) {
    const rows = [];
    lines.forEach((raw, index) => {
      if (index === 1 && isTableRule(raw)) return;
      rows.push(tableCells(raw));
    });
    if (!rows.length) return document.createDocumentFragment();
    const cols = Math.max(1, ...rows.map((row) => row.length));

    const wrap = el("div", "md-table-wrap");
    const table = el("table", "md-table");
    const thead = document.createElement("thead");
    const tbody = document.createElement("tbody");
    rows.forEach((cells, index) => {
      const tr = document.createElement("tr");
      if (index === 0) tr.className = "md-tr-head";
      for (let c = 0; c < cols; c += 1) {
        const cell = document.createElement(index === 0 ? "th" : "td");
        appendInline(cell, cells[c] || "", index === 0 ? "md-b" : "", opts);
        tr.appendChild(cell);
      }
      (index === 0 ? thead : tbody).appendChild(tr);
    });
    table.appendChild(thead);
    table.appendChild(tbody);
    wrap.appendChild(table);
    return wrap;
  }

  /* ── bloki: pętla główna ───────────────────────────────────────────── */

  function render(text, opts) {
    const options = opts || {};
    const frag = document.createDocumentFragment();
    const lines = String(text == null ? "" : text)
      .split("\n")
      .map((line) => line.replace(/\r+$/, ""));

    let i = 0;
    let pendingBlank = false;
    let started = false;

    while (i < lines.length) {
      const raw = lines[i];
      const trimmed = raw.trim();

      if (!trimmed) {
        pendingBlank = started;
        i += 1;
        continue;
      }
      if (pendingBlank) {
        frag.appendChild(el("div", "md-blank"));
        pendingBlank = false;
      }
      started = true;

      const fence = fenceStart(raw);
      if (fence) {
        i += 1;
        const body = [];
        while (i < lines.length) {
          if (isFenceEnd(lines[i], fence.token)) {
            i += 1;
            break;
          }
          body.push(lines[i]);
          i += 1;
        }
        frag.appendChild(codeBlock(body, fence.info, options));
        continue;
      }

      if (isTableRow(raw) && i + 1 < lines.length && isTableRule(lines[i + 1])) {
        const rows = [];
        while (i < lines.length && isTableRow(lines[i])) {
          rows.push(lines[i]);
          i += 1;
        }
        frag.appendChild(tableBlock(rows, options));
        continue;
      }

      const head = heading(trimmed);
      if (head) {
        const row = el("div", head.level <= 3 ? `md-h md-h${head.level}` : "md-h md-hx");
        appendInline(row, head.text, head.level <= 3 ? "md-b" : "md-b md-hx", options);
        frag.appendChild(row);
        i += 1;
        continue;
      }

      if (isRule(trimmed)) {
        frag.appendChild(ruleRow());
        i += 1;
        continue;
      }

      const item = listItem(raw);
      if (item) {
        frag.appendChild(listRow(item, options));
        i += 1;
        continue;
      }

      const quote = quoteLine(raw);
      if (quote) {
        const depth = quote.depth;
        const group = [quote.rest];
        i += 1;
        while (i < lines.length) {
          const next = quoteLine(lines[i]);
          if (!next || next.depth !== depth) break;
          group.push(next.rest);
          i += 1;
        }
        frag.appendChild(quoteBlock(group, depth, options));
        continue;
      }

      // Zwykły akapit: każda linia źródła to osobny wiersz (twarde złamanie).
      const row = el("div", "md-line");
      appendInline(row, trimmed, "", options);
      frag.appendChild(row);
      i += 1;
    }

    return frag;
  }

  global.ArndtMarkdown = { render, inlineText: (text) => inlineSegments(String(text), "", 0) };
})(typeof window !== "undefined" ? window : this);
