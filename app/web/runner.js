/* v0.3: uruchamianie zadania z przeglądarki (app/web/serve.py).
 *
 * UI nie woła agenta bezpośrednio — pyta lokalny serwer, który uruchamia
 * binarkę jako proces potomny. Gdy serwera nie ma (np. strona z `http.server`),
 * kontrolki są wyłączone i widok działa jak dotąd: tylko odczyt.
 */
"use strict";

(function () {
  const form = document.getElementById("runner");
  const taskInput = document.getElementById("task");
  const runBtn = document.getElementById("run");
  const stopBtn = document.getElementById("stop");
  const statusEl = document.getElementById("runner-status");
  if (!form || !taskInput || !runBtn) return;

  const setStatus = (text, tone) => {
    statusEl.textContent = text || "";
    statusEl.className = "runner-status" + (tone ? " " + tone : "");
  };

  const setBusy = (busy, session) => {
    runBtn.disabled = busy;
    stopBtn.hidden = !busy;
    taskInput.disabled = busy;
    if (busy) {
      setStatus("biegnie" + (session ? " · " + session : "") + "…");
    }
  };

  let pollTimer = null;
  let lastSession = null;

  // Wybór sesji robi app.js: klikamy wiersz listy, gdy pojawi się nowa sesja.
  const selectSession = (id) => {
    if (!id) return;
    const row = [...document.querySelectorAll("li.session[data-id]")].find(
      (el) => el.dataset.id === id,
    );
    if (row) row.click();
  };

  const poll = async () => {
    try {
      const res = await fetch("/api/status", { cache: "no-store" });
      if (!res.ok) throw new Error("status " + res.status);
      const st = await res.json();
      setBusy(!!st.running, st.session);
      if (st.running && st.session && st.session !== lastSession) {
        lastSession = st.session;
        selectSession(st.session);
      }
      if (!st.running) {
        if (st.error) setStatus("błąd: " + st.error, "err");
        else if (st.code === 0) setStatus("gotowe" + (st.session ? " · " + st.session : ""), "ok");
        else if (typeof st.code === "number") setStatus("kod wyjścia " + st.code, "err");
      }
      clearTimeout(pollTimer);
      pollTimer = setTimeout(poll, st.running ? 1000 : 4000);
    } catch {
      // Serwera nie ma (strona z `python3 -m http.server`) — tryb tylko do odczytu.
      runBtn.disabled = true;
      setStatus("brak serwera: tylko odczyt", "");
      clearTimeout(pollTimer);
      pollTimer = setTimeout(poll, 8000);
    }
  };

  form.addEventListener("submit", async (ev) => {
    ev.preventDefault();
    const task = taskInput.value.trim();
    if (!task) return;
    runBtn.disabled = true;
    setStatus("wysyłam…");
    try {
      const res = await fetch("/api/run", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ task }),
      });
      const body = await res.json().catch(() => ({}));
      if (res.status === 409) {
        setStatus("zadanie już trwa", "err");
        runBtn.disabled = false;
        return;
      }
      if (!res.ok) {
        setStatus("nie wyszło: " + (body.error || res.status), "err");
        runBtn.disabled = false;
        return;
      }
      taskInput.value = "";
      lastSession = null;
      if (body.session) selectSession(body.session);
      setStatus("uruchomione", "ok");
    } catch {
      setStatus("brak serwera (app/web/serve.py)", "err");
      runBtn.disabled = false;
    }
    clearTimeout(pollTimer);
    pollTimer = setTimeout(poll, 500);
  });

  stopBtn.addEventListener("click", async () => {
    try {
      await fetch("/api/stop", { method: "POST" });
      setStatus("zatrzymuję…");
    } catch {
      setStatus("brak serwera", "err");
    }
  });

  poll();
})();
