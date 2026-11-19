//! Pętla agenta: decyzja -> bramki -> wykonanie -> obserwacja -> warunek stopu.
//!
//! Bramki w kolejności: allowlista narzędzi, scope guard, detektor dryfu.
//! Wywołania narzędzi idą przez perimeter MCP (osobny proces) albo wprocesowo.

use crate::context::{MemoryLevel, Prefix, Session};
use crate::mcp::McpClient;
use crate::provider::{Action, Brain, Turn};
use crate::store::Store;
use crate::tools;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;

pub struct Config {
    pub workspace: PathBuf,
    pub max_steps: usize,
    pub allowlist: Vec<String>,
    pub provider: Option<String>,
    pub quiet: bool,
    pub mcp: bool,
    pub memory: MemoryLevel,
    /// Wznowienie po `crashu`: sesja i kroki z `.agent/resume` + SQLite.
    pub resume: bool,
    /// Runda 14: model wybrany w pickerze / z `.agent/config`.
    /// `ARNDTAI_MODEL` nadal ma pierwszeństwo w `provider`.
    pub model: Option<String>,
    /// Runda 14: `limit.output` modelu przycięty do `MAX_TOKENS`.
    pub max_tokens: Option<usize>,
    /// Runda 15: tryb zgód `ask` — pytanie przed efektem ubocznym.
    /// `None` = `always-approve` (nic się nie zmienia).
    pub approver: Option<Approver>,
    /// Runda 16: skille wczytane przez `/skill <nazwa>` w TUI (nazwa, treść).
    /// Pełna treść wchodzi do kontekstu tylko tędy albo przez `skill.load`;
    /// w CLI i w testach lista jest pusta.
    pub skill_context: Vec<(String, String)>,
}

/// Prośba o zgodę: narzędzie, argument (ścieżka albo kanał) i krok.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalRequest {
    pub step: u64,
    pub tool: String,
    pub arg: String,
}

/// Bramka zgód w trybie `ask`. Agent wysyła prośbę i **czeka** na decyzję —
/// TUI rysuje pytanie w composerze, a wątek pętli stoi do odpowiedzi.
pub struct Approver {
    requests: Sender<ApprovalRequest>,
    decisions: Mutex<Receiver<bool>>,
}

impl Approver {
    /// Para końców: agent dostaje [`Approver`], TUI — [`ApprovalHandle`].
    pub fn channel() -> (Approver, ApprovalHandle) {
        let (req_tx, req_rx) = mpsc::channel();
        let (dec_tx, dec_rx) = mpsc::channel();
        (
            Approver {
                requests: req_tx,
                decisions: Mutex::new(dec_rx),
            },
            ApprovalHandle {
                requests: req_rx,
                decisions: dec_tx,
            },
        )
    }

    /// `true` = zgoda. Brak odpowiedzi (TUI padło) to odmowa — bezpieczna
    /// strona: bez zgody nie ma efektu.
    pub(crate) fn ask(&self, request: ApprovalRequest) -> bool {
        if self.requests.send(request).is_err() {
            return false;
        }
        match self.decisions.lock() {
            Ok(rx) => rx.recv().unwrap_or(false),
            Err(_) => false,
        }
    }
}

/// Koniec TUI: odbiera prośby i odsyła decyzję (`true` = zgoda).
pub struct ApprovalHandle {
    requests: Receiver<ApprovalRequest>,
    decisions: Sender<bool>,
}

impl ApprovalHandle {
    /// Prośba czekająca na odpowiedź (bez blokowania).
    pub fn try_recv(&self) -> Option<ApprovalRequest> {
        self.requests.try_recv().ok()
    }

    /// Prośba czekająca na odpowiedź (blokuje) — pętla bez UI i testy.
    #[cfg(test)]
    pub fn recv(&self) -> Option<ApprovalRequest> {
        self.requests.recv().ok()
    }

    pub fn answer(&self, allow: bool) {
        let _ = self.decisions.send(allow);
    }
}


pub struct Report {
    pub status: String,
    pub steps: usize,
    pub tool_calls: usize,
    pub denied: usize,
    pub drift: usize,
    pub reason: String,
    pub session: PathBuf,
}

struct Outcome {
    status: &'static str,
    reason: String,
    steps: usize,
    tool_calls: usize,
    denied: usize,
    drift: usize,
}

/// Sekcja kontekstu ze skillami wczytanymi `/skill`: nazwa + treść (limit
/// 8 KB na skill, jak w `skill.load`, z jawnym ostrzeżeniem przy obcięciu).
pub(crate) fn render_skill_context(skills: &[(String, String)]) -> String {
    let mut out = String::from("\n=== SKILLE ZALADOWANE DO KONTEKSTU ===\n");
    for (name, content) in skills {
        let (text, truncated) = crate::skills::clip(content, crate::skills::MAX_CONTENT_BYTES);
        out.push_str(&format!("--- {} ---\n{}\n", name, text.trim_end()));
        if truncated {
            out.push_str(&format!(
                "(tresc obcieta do {} bajtow)\n",
                crate::skills::MAX_CONTENT_BYTES
            ));
        }
    }
    out
}

fn emit(cfg: &Config, stage: &str, detail: &str) {
    if !cfg.quiet {
        println!("  [{}] {}", stage, detail);
    }
}

fn short(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let clipped: String = text.chars().take(limit).collect();
        format!("{}...", clipped)
    }
}

pub fn run(task: &str, cfg: &Config) -> Report {
    let prefix = Prefix::load(&cfg.workspace, cfg.memory);
    let mut rendered = prefix.render();
    // Runda 16: treść skilli wczytanych `/skill` — tylko one, nigdy cała lista.
    if !cfg.skill_context.is_empty() {
        rendered.push_str(&render_skill_context(&cfg.skill_context));
    }
    // Runda 11 (M1): poziom `pro` (i `ultra`) dokłada do kontekstu trafienia
    // z wlasnej historii — FTS po `transcripts`, nie po plikach JSONL.
    if matches!(cfg.memory, MemoryLevel::Pro | MemoryLevel::Ultra) {
        rendered.push_str(&history_section(&cfg.workspace, task));
    }
    let transport = if cfg.mcp { "mcp" } else { "wprocesowy" };

    // F8: wznowienie musi trafic w te sama sesje, inaczej (session_id, step_id)
    // nigdy sie nie spotka i `done` nie zostanie odczytane.
    let resume_id = if cfg.resume {
        crate::read_resume_pointer(&cfg.workspace)
    } else {
        None
    };
    if cfg.resume && resume_id.is_none() {
        let session = Session::start(&cfg.workspace, task);
        session.event(
            "error",
            json!({ "message": "brak .agent/resume — nie ma czego wznawiac" }),
        );
        return finish(
            cfg,
            &session,
            transport,
            Outcome {
                status: "FAILED_ERROR",
                reason: "brak .agent/resume — nie ma czego wznawiac".to_string(),
                steps: 0,
                tool_calls: 0,
                denied: 0,
                drift: 0,
            },
        );
    }
    let session = match resume_id {
        Some(id) => Session::resume(&cfg.workspace, &id),
        None => Session::start(&cfg.workspace, task),
    };
    crate::write_resume_pointer(&cfg.workspace, &session.id);

    // Faza B: SQLite store — idempotencja + resume. Brak store = praca bez
    // persistencji (ostrzezenie, nie twardy blad, zeby demo nie padalo).
    let store = Store::for_workspace(&cfg.workspace);
    let store_ok = match store.init().and_then(|_| store.create_session(&session.id, task)) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("ostrzezenie: SQLite niedostepny ({}), lece bez resume", e);
            false
        }
    };

    if store_ok {
        // Runda 11 (M1): wiersz zadania. `--resume` trafia w ten sam `id` i
        // status wraca na `running`, tekst zadania zostaje pierwotny.
        if let Err(e) = store.start_task(&session.id, &session.id, task) {
            session.event("store_error", json!({ "stage": "task", "message": e }));
        }
        session.event(
            "store",
            json!({
                "path": ".agent/state.db",
                "journal_mode": store.journal_mode().unwrap_or_else(|e| format!("error: {}", e)),
                "busy_timeout": store.busy_timeout().unwrap_or(-1),
            }),
        );
    }

    // Runda 10: granica tury. Nowa tura liczy kroki od `max(step_id)+1`, a tura
    // przerwana crashem jest kontynuowana od swojego pierwszego kroku — tylko
    // w jej obrebie wolno odtworzyc wynik i tylko tam `step_id` jest zajęty.
    let mut turn_no: i64 = 0;
    let mut turn_base: u64 = 0;
    if store_ok {
        match store.open_turn(&session.id) {
            Ok(turn) => {
                turn_no = turn.turn_no;
                turn_base = turn.base_step;
            }
            Err(e) => {
                session.event("store_error", json!({ "stage": "turn", "message": e }));
            }
        }
    }

    let mut brain = Brain::from_env_full(
        cfg.provider.as_deref(),
        task,
        &cfg.allowlist,
        cfg.model.as_deref(),
        cfg.max_tokens,
    );
    // Runda 16: lista `- nazwa: opis` (maks. 20) idzie do promptu systemowego;
    // mock jej nie wysyła, więc dla trybu offline to pusty krok.
    brain.set_skills(crate::skills::prompt_lines(&crate::skills::discover(
        &cfg.workspace,
    )));
    let brain_label = brain.label();
    let brain_note = brain.note().map(|s| s.to_string());
    if !cfg.quiet {
        println!("ArndtAi — agent wykonawczy");
        println!("Zadanie: {}", task);
        println!("Folder: {}", cfg.workspace.display());
        println!("Mozg: {}", brain_label);
        if let Some(note) = brain_note.as_deref() {
            println!("Uwaga: {}", note);
        }
        println!("Perimetr: {}", transport);
        println!("Poziom pamieci: {}", cfg.memory.as_str());
        println!();
    }

    let mut client = if cfg.mcp {
        match McpClient::spawn(&cfg.workspace) {
            Ok(c) => Some(c),
            Err(e) => {
                let reason = format!("perimeter MCP nie wystartowal: {}", e);
                emit(cfg, "FAILED_ERROR", &reason);
                session.event("error", json!({ "message": reason.clone() }));
                if store_ok && turn_no > 0 {
                    let _ = store.close_turn(&session.id, turn_no);
                }
                return finish(
                    cfg,
                    &session,
                    transport,
                    Outcome {
                        status: "FAILED_ERROR",
                        reason,
                        steps: 0,
                        tool_calls: 0,
                        denied: 0,
                        drift: 0,
                    },
                );
            }
        }
    } else {
        None
    };

    let allowlist = cfg.allowlist.clone();
    let mut turns: Vec<Turn> = Vec::new();
    let mut steps: usize = 0;
    let mut tool_calls: usize = 0;
    let mut denied: usize = 0;
    let mut drift: usize = 0;
    let mut streak: usize = 0;
    // Odcisk (tool+args) ostatnich wykonanych wywolan; okno = biezace + 3 poprzednie.
    let mut recent: Vec<String> = Vec::new();

    let outcome: Outcome = 'steps: loop {
        if steps >= cfg.max_steps {
            break 'steps Outcome {
                status: "FAILED_BUDGET",
                reason: format!("budzet krokow wyczerpany ({})", cfg.max_steps),
                steps,
                tool_calls,
                denied,
                drift,
            };
        }
        steps += 1;
        // Numer kroku jest ciagly w obrebie sesji (runda 10); `steps` liczy
        // tylko biezaca ture i nadal pilnuje budzetu `--steps`.
        // `turn_base` to pierwszy `step_id` tej tury, `steps` jest 1-based.
        let step_no = turn_base.saturating_sub(1) + steps as u64;

        emit(cfg, "RECEIVED", &format!("krok {}", step_no));
        // Runda 21: przyrosty strumienia idą do TUI osobnym kanałem (poza
        // transkryptem); bez odbiorcy callback nie powstaje i nic nie drukujemy.
        let mut on_delta = |channel: crate::provider::Channel, text: &str| {
            crate::provider::publish_delta(step_no, channel, text);
        };
        let progress: Option<&mut dyn FnMut(crate::provider::Channel, &str)> =
            if crate::provider::delta_sink_active() {
                Some(&mut on_delta)
            } else {
                None
            };
        let result = brain.next(task, &rendered, &turns, progress);
        // Runda 12: kazda proba naprawy zostaje w transkrypcie (JSONL + SQLite),
        // takze wtedy, gdy zadanie konczy sie DONE.
        for notice in brain.take_repairs() {
            emit(cfg, "REPAIR", &notice);
            session.event("repair", json!({ "step": step_no, "detail": notice }));
        }
        let decision = match result {
            Ok(d) => d,
            Err(e) => {
                emit(cfg, "FAILED_ERROR", &e);
                session.event("error", json!({ "message": e.clone() }));
                break 'steps Outcome {
                    status: "FAILED_ERROR",
                    reason: e,
                    steps,
                    tool_calls,
                    denied,
                    drift,
                };
            }
        };
        emit(cfg, "PLANNING", &decision.thought);
        session.event(
            "thought",
            json!({ "step": step_no, "thought": decision.thought }),
        );

        match decision.action {
            Action::Finish { summary } => {
                emit(cfg, "DONE", &summary);
                break 'steps Outcome {
                    status: "DONE",
                    reason: summary,
                    steps,
                    tool_calls,
                    denied,
                    drift,
                };
            }
            Action::Tool { name, args } => {
                emit(cfg, "STEP_PROPOSED", &name);
                emit(cfg, "POLICY_CHECK", &name);
                let allowed = allowlist.iter().any(|a| a == &name);

                emit(cfg, "SCOPE_CHECK", &name);
                let scope_error = match tools::path_arg(&args) {
                    Some(rel) => tools::resolve(&cfg.workspace, &rel).err().map(|e| e.summary()),
                    None => None,
                };

                // Runda 9: dryf liczy sie tylko dla narzedzi zmieniajacych stan.
                // `fs.read` jest read-only — nie wchodzi do okna i nie moze
                // wywolac odmowy; dla niego odmowe daje tylko scope guard.
                let mutating = tools::is_mutating(&name);
                let fingerprint = format!("{}|{}", name, args);
                // F11: dryf to powtorzenie odcisku w oknie ostatnich 4 mutacji,
                // nie tylko bezposrednio po poprzedniej (A,B,A,B tez jest dryfem).
                let repeats = recent.iter().filter(|f| *f == &fingerprint).count() + 1;
                let is_drift = mutating && repeats >= 2;

                let denial = if !allowed {
                    Some(format!("{} nie jest na allowliscie", name))
                } else if let Some(e) = scope_error {
                    Some(format!("{} odrzucone przez bramke zakresu: {}", name, e))
                } else if is_drift {
                    Some(format!(
                        "{} powtarza odcisk (tool+args) w oknie 4 wywolan bez zmiany (dryf)",
                        name
                    ))
                } else {
                    None
                };

                if let Some(message) = denial {
                    denied += 1;
                    streak += 1;
                    if is_drift {
                        drift += 1;
                    }
                    emit(cfg, "OBSERVE", &format!("odmowa: {}", message));
                    session.event(
                        "denied",
                        json!({ "step": step_no, "tool": name, "reason": message, "drift": is_drift }),
                    );
                    turns.push(Turn {
                        role: "tool".to_string(),
                        content: format!("{} ok=false -> {}", name, message),
                    });
                    if streak >= 3 {
                        break 'steps Outcome {
                            status: "FAILED_STALLED",
                            reason: format!("{} odmow z rzedu", streak),
                            steps,
                            tool_calls,
                            denied,
                            drift,
                        };
                    }
                    continue 'steps;
                }

                // Runda 15: tryb `ask`. Pytanie stoi PRZED zapisem `proposed`,
                // żeby odmowa nie zostawiła kroku, który po crashu wygląda jak
                // przerwany przebieg. Odmowa liczy się do licznika odmów, ale
                // nie do serii `FAILED_STALLED` — użytkownik ma prawo odmówić
                // i nie zabija to sesji.
                if let Some(approver) = cfg.approver.as_ref() {
                    if tools::is_mutating(&name) {
                        let arg = tools::path_arg(&args)
                            .or_else(|| {
                                args.get("channel")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_default();
                        let allowed_now = approver.ask(ApprovalRequest {
                            step: step_no,
                            tool: name.clone(),
                            arg: arg.clone(),
                        });
                        if !allowed_now {
                            denied += 1;
                            let message = format!("{}: odmowa uzytkownika (tryb ask)", name);
                            emit(cfg, "OBSERVE", &format!("odmowa: {}", message));
                            session.event(
                                "denied",
                                json!({
                                    "step": step_no,
                                    "tool": name,
                                    "reason": "USER_DENIED",
                                    "drift": false
                                }),
                            );
                            turns.push(Turn {
                                role: "tool".to_string(),
                                content: format!(
                                    "{} ok=false -> USER_DENIED: uzytkownik nie zezwolil na {}",
                                    name, arg
                                ),
                            });
                            continue 'steps;
                        }
                    }
                }

                // F2/F4/F8: przed efektem ubocznym krok jest `proposed`; jesli
                // wiersz juz istnieje — replay wyniku albo skip po crashu.
                let step_id = step_no;
                let args_json = args.to_string();
                let mut replay: Option<String> = None;
                let mut abandoned = false;
                // Przy transporcie MCP kroki zapisuje serwer perimetru (ten sam
                // `state.db`); rdzen nie dubluje wlasnego `proposed`, bo to
                // wygladaloby jak crash i krok zostalby pominiety.
                if store_ok && !cfg.mcp {
                    // Najpierw czytamy wiersz SPRZED tego przebiegu: swiezo
                    // wstawiony `proposed` nie jest crashem.
                    let prior = store.fetch_step(&session.id, step_id);
                    match prior {
                        Ok(Some(row)) => {
                            let same = row.tool == name && row.args_json == args_json;
                            if same && row.status == "done" {
                                replay = Some(row.result_json.unwrap_or_else(|| "{}".to_string()));
                            } else if row.status == "proposed" || (same && row.status == "abandoned")
                            {
                                let note =
                                    json!({ "skipped": "crash_before_result" }).to_string();
                                if row.status == "proposed" {
                                    if let Err(e) =
                                        store.mark_abandoned(&session.id, step_id, &note)
                                    {
                                        session.event(
                                            "store_error",
                                            json!({ "stage": "abandoned", "message": e }),
                                        );
                                    }
                                }
                                abandoned = true;
                            } else if let Err(e) =
                                store.proposed(&session.id, step_id, &name, &args_json)
                            {
                                session.event(
                                    "store_error",
                                    json!({ "stage": "proposed", "message": e }),
                                );
                            }
                        }
                        Ok(None) => {
                            if let Err(e) = store.proposed(&session.id, step_id, &name, &args_json)
                            {
                                session.event(
                                    "store_error",
                                    json!({ "stage": "proposed", "message": e }),
                                );
                            }
                        }
                        Err(e) => {
                            session.event("store_error", json!({ "stage": "fetch", "message": e }))
                        }
                    }
                }

                if let Some(cached) = replay {
                    emit(cfg, "OBSERVE", &format!("{} cached (krok {} juz wykonany)", name, step_id));
                    session.event(
                        "tool_result_cached",
                        json!({ "step": step_no, "tool": name, "result": cached }),
                    );
                    turns.push(Turn {
                        role: "tool".to_string(),
                        content: format!("{} ok=true -> {} (z zapisanego kroku)", name, cached),
                    });
                    continue 'steps;
                }

                if abandoned {
                    emit(cfg, "OBSERVE", &format!("{} skipped (crash przed wynikiem)", name));
                    session.event(
                        "tool_result_skipped",
                        json!({ "step": step_no, "tool": name, "reason": "crash_before_result" }),
                    );
                    turns.push(Turn {
                        role: "tool".to_string(),
                        content: format!(
                            "{} ok=false -> SKIPPED: krok przerwany przed wynikiem, nie powtarzam (at-most-once)",
                            name
                        ),
                    });
                    continue 'steps;
                }

                emit(cfg, "EXECUTING", &name);
                session.event("tool_call", json!({ "step": step_no, "tool": name, "args": args }));

                let ctx = tools::CallCtx {
                    task_id: session.id.clone(),
                    session_id: session.id.clone(),
                    step_id: step_no,
                };
                let result = match client.as_mut() {
                    Some(c) => match c.call(&name, &args, &ctx) {
                        Ok(r) => r,
                        Err(e) => tools::ToolResult::error("IO_ERROR", e),
                    },
                    None => tools::call(&name, &args, &cfg.workspace),
                };
                if store_ok && !cfg.mcp {
                    let result_json = json!({
                        "ok": result.ok,
                        "code": result.code,
                        "bytes": result.bytes,
                        "output": result.output,
                    })
                    .to_string();
                    let persisted = if name == "notify.send" && result.ok {
                        store.complete_with_outbox(&session.id, step_id, &result_json, &args_json)
                    } else {
                        store.complete(&session.id, step_id, &result_json)
                    };
                    if let Err(e) = persisted {
                        session.event("store_error", json!({ "stage": "complete", "message": e }));
                    }
                }
                tool_calls += 1;
                // Tylko mutacje trafiaja do okna dryfu; odczyty go nie zapychaja.
                if mutating {
                    recent.push(fingerprint);
                    if recent.len() > 3 {
                        recent.remove(0);
                    }
                }
                if result.ok {
                    streak = 0;
                } else {
                    streak += 1;
                }

                emit(
                    cfg,
                    "OBSERVE",
                    &format!("{} ok={} ({} bajtow)", name, result.ok, result.bytes),
                );
                session.event(
                    "tool_result",
                    json!({
                        "step": step_no,
                        "tool": name,
                        "ok": result.ok,
                        "code": result.code,
                        "bytes": result.bytes,
                        "output": result.output,
                    }),
                );
                turns.push(Turn {
                    role: "tool".to_string(),
                    content: format!(
                        "{} ok={} -> {}",
                        name,
                        result.ok,
                        short(&result.summary(), 600)
                    ),
                });

                if streak >= 3 {
                    break 'steps Outcome {
                        status: "FAILED_STALLED",
                        reason: format!("{} nieudanych krokow z rzedu", streak),
                        steps,
                        tool_calls,
                        denied,
                        drift,
                    };
                }
                emit(cfg, "STOP_CHECK", "kontynuuje");
            }
        }
    };

    if let Some(mut c) = client.take() {
        c.shutdown();
    }

    // Tura zamknieta = jej kroki staja sie historia. Wznowienie po tym miejscu
    // zaczyna nowa ture od `max(step_id)+1`, bez replayu.
    if store_ok && turn_no > 0 {
        if let Err(e) = store.close_turn(&session.id, turn_no) {
            session.event("store_error", json!({ "stage": "turn_done", "message": e }));
        }
    }

    finish(cfg, &session, transport, outcome)
}

/// Sekcja kontekstu dla poziomu `pro`: trafienia FTS z wlasnej historii.
/// Puste (brak bazy, brak trafien, puste zapytanie) = pusty napis, bez bledu.
fn history_section(workspace: &Path, task: &str) -> String {
    let store = Store::for_workspace(workspace);
    let hits = store
        .init()
        .and_then(|_| store.search_transcripts(task))
        .unwrap_or_default();
    if hits.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n=== PAMIEC: TRAFIENIA Z HISTORII (FTS) ===\n");
    for hit in hits {
        out.push_str(&format!(
            "- sesja {} krok {}: {}\n",
            hit.session_id, hit.step, hit.snippet
        ));
    }
    out
}

/// Status zadania w `tasks` jezykiem statusow sesji: `done` / `failed`.
fn task_status(report_status: &str) -> &'static str {
    if report_status == "DONE" {
        "done"
    } else {
        "failed"
    }
}

fn finish(cfg: &Config, session: &Session, transport: &str, outcome: Outcome) -> Report {
    session.event(
        "summary",
        json!({
            "status": outcome.status,
            "steps": outcome.steps,
            "tool_calls": outcome.tool_calls,
            "denied": outcome.denied,
            "drift": outcome.drift,
            "reason": outcome.reason,
        }),
    );
    // Runda 11 (M1): zadanie dostaje status koncowy razem z podsumowaniem.
    // Brak wiersza (np. przebieg bez store) nie jest bledem przebiegu.
    let store = Store::for_workspace(&cfg.workspace);
    let _ = store.set_task_status(&session.id, task_status(outcome.status));
    if !cfg.quiet {
        println!();
        println!("Status: {}", outcome.status);
        println!("Kroki: {}", outcome.steps);
        println!("Narzedzia: {}", outcome.tool_calls);
        println!("Odmowy: {}", outcome.denied);
        println!("Dryf: {}", outcome.drift);
        println!("Powod: {}", outcome.reason);
        println!("Perimetr: {}", transport);
        println!("Transkrypt: {}", session.file.display());
    }
    Report {
        status: outcome.status.to_string(),
        steps: outcome.steps,
        tool_calls: outcome.tool_calls,
        denied: outcome.denied,
        drift: outcome.drift,
        reason: outcome.reason,
        session: session.file.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::fs;

    fn unique_suffix() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        format!("{}-{}", n, std::process::id())
    }

    fn temp_ws(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-agent-{}-{}-{}",
            tag,
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn config(workspace: &PathBuf, resume: bool) -> Config {
        Config {
            workspace: workspace.clone(),
            max_steps: 8,
            allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
            provider: Some("mock".to_string()),
            quiet: true,
            mcp: false,
            memory: MemoryLevel::Lite,
            resume,
            model: None,
            max_tokens: None,
            approver: None,
            skill_context: Vec::new(),
        }
    }

    fn transcript_has(session_file: &PathBuf, needle: &str) -> bool {
        fs::read_to_string(session_file)
            .map(|s| s.contains(needle))
            .unwrap_or(false)
    }

    fn events_of(session_file: &PathBuf) -> Vec<serde_json::Value> {
        fs::read_to_string(session_file)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn tool_call_steps(session_file: &PathBuf) -> Vec<u64> {
        events_of(session_file)
            .iter()
            .filter(|e| e["kind"] == json!("tool_call"))
            .filter_map(|e| e["data"]["step"].as_u64())
            .collect()
    }

    fn has_kind(session_file: &PathBuf, kind: &str) -> bool {
        events_of(session_file)
            .iter()
            .any(|e| e["kind"] == json!(kind))
    }

    #[test]
    fn step_is_persisted_after_run() {
        let ws = temp_ws("persist");
        let report = run("write out.txt :: alfa", &config(&ws, false));
        assert_eq!(report.status, "DONE", "{}", report.reason);
        assert_eq!(fs::read_to_string(ws.join("out.txt")).unwrap(), "alfa");

        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        let id = crate::read_resume_pointer(&ws).expect("wskaznik .agent/resume");
        let steps = store.steps_for_session(&id).unwrap();
        assert_eq!(steps.len(), 1, "krok musi trafic do tabeli steps");
        assert_eq!(steps[0].status, "done");
        assert!(steps[0].result_json.is_some(), "wynik kroku musi byc zapisany");
        let _ = fs::remove_dir_all(&ws);
    }

    /// At-most-once w obrebie jednej tury (runda 10 tez): tura przerwana
    /// crashem wznawia sie od swojego `base_step`, krok `done` jest odtwarzany
    /// z zapisanego wyniku bez powtarzania efektu.
    #[test]
    fn crashed_turn_replays_its_done_step_without_repeating_effect() {
        let ws = temp_ws("resume-done");
        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        store
            .create_session("sesja-crash", "write out.txt :: alfa")
            .unwrap();
        let turn = store.open_turn("sesja-crash").unwrap();
        assert_eq!(turn.base_step, 1, "pierwsza tura startuje od kroku 1");
        store
            .proposed(
                "sesja-crash",
                1,
                "fs.write",
                &json!({"content":"alfa","path":"out.txt"}).to_string(),
            )
            .unwrap();
        store.complete("sesja-crash", 1, "{\"ok\":true}").unwrap();
        fs::create_dir_all(ws.join(".agent")).unwrap();
        fs::write(ws.join(".agent/resume"), "sesja-crash").unwrap();

        let report = run("write out.txt :: alfa", &config(&ws, true));
        assert_eq!(report.status, "DONE", "{}", report.reason);
        assert!(
            !ws.join("out.txt").exists(),
            "krok `done` nie moze wykonac efektu drugi raz (at-most-once)"
        );
        assert!(
            transcript_has(&report.session, "tool_result_cached"),
            "w tej samej turze wynik idzie z zapisanego kroku"
        );
        let steps = store.steps_for_session("sesja-crash").unwrap();
        assert_eq!(steps.len(), 1, "bez duplikatu kroku");
        assert_eq!(
            store.turns_for_session("sesja-crash").unwrap()[0].status,
            "done",
            "tura zamknieta po przebiegu"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn resume_marks_proposed_step_abandoned_without_executing() {
        let ws = temp_ws("resume-proposed");
        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        store.create_session("sesja-proposed", "write out.txt :: alfa").unwrap();
        // Crash w obrebie tury: tura zostaje `running`, krok `proposed` bez wyniku.
        store.open_turn("sesja-proposed").unwrap();
        store
            .proposed(
                "sesja-proposed",
                1,
                "fs.write",
                &json!({"content":"alfa","path":"out.txt"}).to_string(),
            )
            .unwrap();
        fs::create_dir_all(ws.join(".agent")).unwrap();
        fs::write(ws.join(".agent/resume"), "sesja-proposed").unwrap();

        let report = run("write out.txt :: alfa", &config(&ws, true));
        assert_eq!(report.status, "DONE", "{}", report.reason);
        assert!(
            !ws.join("out.txt").exists(),
            "krok 'proposed' po crashu nie moze wykonac sie automatycznie (at-most-once)"
        );
        assert!(
            transcript_has(&report.session, "tool_result_skipped"),
            "pominiecie musi byc widoczne w transkrypcie"
        );
        let steps = store.steps_for_session("sesja-proposed").unwrap();
        assert_eq!(steps[0].status, "abandoned");
        let _ = fs::remove_dir_all(&ws);
    }

    /// Runda 10: po zakonczonej turze to samo wywolanie z innymi argumentami
    /// jest nowym krokiem (bez DENIED), a nie powtorka historii.
    #[test]
    fn new_turn_can_reuse_a_call_with_other_arguments() {
        let ws = temp_ws("args-after-history");
        let first = run("write a.txt :: alfa", &config(&ws, false));
        assert_eq!(first.status, "DONE", "{}", first.reason);
        let second = run("write a.txt :: beta", &config(&ws, true));
        assert_eq!(second.status, "DONE", "{}", second.reason);
        assert_eq!(second.denied, 0, "historia nie moze dawac DENIED");
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "beta");
        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        let id = crate::read_resume_pointer(&ws).unwrap();
        let ids: Vec<i64> = store
            .steps_for_session(&id)
            .unwrap()
            .iter()
            .map(|r| r.step_id)
            .collect();
        assert_eq!(ids, vec![1, 2], "nowy krok dostaje nowy numer");
        let _ = fs::remove_dir_all(&ws);
    }

    /// Runda 9: odczyt nie jest dryfem. Ta sama sekwencja na tym samym pliku
    /// (`fs.read -> fs.write -> fs.read`) musi przejsc bez odmowy.
    #[test]
    fn read_write_read_has_no_drift() {
        let ws = temp_ws("read-write-read");
        fs::write(ws.join("a.txt"), "alfa").unwrap();
        let report = run(
            "read a.txt; write a.txt :: beta; read a.txt",
            &config(&ws, false),
        );
        assert_eq!(report.status, "DONE", "{}", report.reason);
        assert_eq!(report.denied, 0, "zadnej odmowy dryfu");
        assert_eq!(report.drift, 0, "dryf: 0");
        assert_eq!(report.tool_calls, 3, "wszystkie trzy odczyty/zapis przeszly");
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "beta");
        let _ = fs::remove_dir_all(&ws);
    }

    /// Mutacja powtorzona w oknie 4 nadal jest dryfem.
    #[test]
    fn drift_detects_alternating_mutation_in_window() {
        let ws = temp_ws("drift-window");
        let report = run(
            "write a.txt :: 1; write b.txt :: 2; write a.txt :: 1; write b.txt :: 2",
            &config(&ws, false),
        );
        assert!(
            report.drift >= 1,
            "A,B,A,B mutacji musi byc dryfem (drift={})",
            report.drift
        );
        assert!(
            report.tool_calls < 4,
            "dryf musi zatrzymac powtorzone wywolanie (tool_calls={})",
            report.tool_calls
        );
        let _ = fs::remove_dir_all(&ws);
    }

    /// Ten sam zapis dwa razy w oknie: odmowa, plik zostaje z pierwszego razu.
    #[test]
    fn repeated_write_is_still_drift() {
        let ws = temp_ws("write-drift");
        let report = run(
            "write a.txt :: alfa; write a.txt :: alfa",
            &config(&ws, false),
        );
        assert_eq!(report.status, "DONE", "{}", report.reason);
        assert_eq!(report.drift, 1, "powtorzona mutacja to dryf");
        assert_eq!(report.denied, 1);
        assert_eq!(report.tool_calls, 1, "drugi zapis nie wykonuje sie");
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "alfa");
        let _ = fs::remove_dir_all(&ws);
    }

    /// `fs.append` to mutacja: dwa razy ta sama tresc = dryf, ale append + read
    /// (nawet na tym samym pliku) = brak dryfu.
    #[test]
    fn append_twice_drifts_but_append_then_read_does_not() {
        let ws = temp_ws("append-drift");
        let drifty = run("append a.txt :: x; append a.txt :: x", &config(&ws, false));
        assert_eq!(drifty.status, "DONE", "{}", drifty.reason);
        assert_eq!(drifty.drift, 1, "powtorzony append to mutacja");
        assert_eq!(drifty.tool_calls, 1, "drugi append nie wykonuje sie");
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "x");

        let ws2 = temp_ws("append-read");
        let clean = run("append b.txt :: y; read b.txt", &config(&ws2, false));
        assert_eq!(clean.status, "DONE", "{}", clean.reason);
        assert_eq!(clean.denied, 0, "brak odmowy");
        assert_eq!(clean.drift, 0, "append + read to nie dryf");
        assert_eq!(fs::read_to_string(ws2.join("b.txt")).unwrap(), "y");
        let _ = fs::remove_dir_all(&ws);
        let _ = fs::remove_dir_all(&ws2);
    }

    /// `fs.append` poza workspace: odmowa scope guardu, plik nie powstaje.
    #[test]
    fn append_outside_workspace_is_denied_and_creates_nothing() {
        let ws = temp_ws("append-scope");
        let name = format!("escape-{}.txt", unique_suffix());
        let task = format!("append ../{} :: nie powinno powstac", name);
        let report = run(&task, &config(&ws, false));
        assert!(report.denied >= 1, "scope guard musi odmowic");
        assert_eq!(report.drift, 0, "to bramka zakresu, nie dryf");
        assert!(
            !ws.parent().unwrap().join(&name).exists(),
            "plik poza workspace nie moze powstac"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn notify_writes_outbox_row_in_step_transaction() {
        let ws = temp_ws("outbox");
        let report = run("notify hej", &config(&ws, false));
        assert_eq!(report.status, "DONE", "{}", report.reason);
        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        let id = crate::read_resume_pointer(&ws).unwrap();
        let rows = store.outbox_for_session(&id).unwrap();
        assert_eq!(rows.len(), 1, "notify.send musi trafic do tabeli outbox");
        assert!(rows[0].contains("hej"));
        let steps = store.steps_for_session(&id).unwrap();
        assert_eq!(steps[0].status, "done");
        assert_eq!(steps[0].tool, "notify.send");
        let _ = fs::remove_dir_all(&ws);
    }

    /// Runda 10: nowa tura po zakonczonej turze nie zaczyna sie od kroku 1.
    /// Pierwszy krok = `max(step_id w sesji) + 1`, w SQLite i w evencie JSONL.
    #[test]
    fn new_turn_continues_the_session_numbering() {
        let ws = temp_ws("turn-numbering");
        let first = run(
            "read a.txt; write a.txt :: beta; read a.txt",
            &config(&ws, false),
        );
        assert_eq!(first.status, "DONE", "{}", first.reason);
        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        let id = crate::read_resume_pointer(&ws).expect("wskaznik sesji");
        let ids = |s: &Store| {
            s.steps_for_session(&id)
                .unwrap()
                .iter()
                .map(|r| r.step_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&store), vec![1, 2, 3], "pierwsza tura: kroki 1..3");

        let second = run("read a.txt", &config(&ws, true));
        assert_eq!(second.status, "DONE", "{}", second.reason);
        assert_eq!(
            ids(&store),
            vec![1, 2, 3, 4],
            "pierwszy krok nowej tury to max+1"
        );
        assert_eq!(
            tool_call_steps(&second.session).last().copied(),
            Some(4),
            "event JSONL liczy dalej, bez powrotu do 1"
        );
        assert!(
            !has_kind(&second.session, "tool_result_cached"),
            "kroki z poprzedniej tury nie moga byc odtwarzane"
        );
    }

    /// Runda 10: powtorzone wywolanie z poprzedniej tury wykonuje sie naprawde —
    /// swiezy `fs.read` widzi aktualna tresc, nie zapamietany blad.
    #[test]
    fn old_result_is_not_replayed_in_a_new_turn() {
        let ws = temp_ws("fresh-read");
        let first = run("read a.txt", &config(&ws, false));
        assert_eq!(first.status, "DONE", "{}", first.reason);
        assert!(!ws.join("a.txt").exists(), "pierwszy odczyt: brak pliku");
        fs::write(ws.join("a.txt"), "swieza tresc").unwrap();

        let second = run("read a.txt", &config(&ws, true));
        assert_eq!(second.status, "DONE", "{}", second.reason);
        let events = events_of(&second.session);
        let last = events
            .iter()
            .rev()
            .find(|e| e["kind"] == json!("tool_result"))
            .expect("wynik fs.read");
        assert_eq!(last["data"]["ok"], json!(true), "odczyt musi widziec plik: {}", last);
        assert!(
            last["data"]["output"]["content"]
                .as_str()
                .unwrap_or("")
                .contains("swieza tresc"),
            "wynik musi byc swiezy: {}",
            last
        );
        assert!(
            !has_kind(&second.session, "tool_result_cached"),
            "poprzednia tura to historia, nie cache"
        );
    }

    /// Runda 10: `--resume` po zapisie dopisuje obie tury do tego samego pliku.
    #[test]
    fn resume_after_append_keeps_both_turns() {
        let ws = temp_ws("resume-append");
        let first = run("append a.txt :: alfa", &config(&ws, false));
        assert_eq!(first.status, "DONE", "{}", first.reason);
        let second = run("append a.txt :: beta", &config(&ws, true));
        assert_eq!(second.status, "DONE", "{}", second.reason);
        assert_eq!(fs::read_to_string(ws.join("a.txt")).unwrap(), "alfabeta");
        assert_eq!(second.denied, 0, "zero odmow");
        assert_eq!(second.drift, 0, "dryf: 0");
        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        let id = crate::read_resume_pointer(&ws).unwrap();
        let ids: Vec<i64> = store
            .steps_for_session(&id)
            .unwrap()
            .iter()
            .map(|r| r.step_id)
            .collect();
        assert_eq!(ids, vec![1, 2], "dopisanie z nowej tury to nowy krok");
        let _ = fs::remove_dir_all(&ws);
    }

    /// Ta sama tura nie moze wygenerowac dwoch roznych wywolan z jednym `step_id`
    /// (ta regula zostaje w mocy — patrz `mcp::tests::repeated_step_id_...`).
    #[test]
    fn one_turn_never_reuses_a_step_id() {
        let ws = temp_ws("unique-ids");
        let report = run(
            "write a.txt :: 1; write b.txt :: 2; append c.txt :: 3",
            &config(&ws, false),
        );
        assert_eq!(report.status, "DONE", "{}", report.reason);
        assert_eq!(tool_call_steps(&report.session), vec![1, 2, 3]);
        let _ = fs::remove_dir_all(&ws);
    }

    /// Runda 11 (M1): poziom `pro` dokłada do kontekstu trafienia z historii
    /// (FTS po `transcripts`); puste zapytanie nie dokłada nic i nie bladzi.
    #[test]
    fn pro_level_appends_history_hits_to_context() {
        let ws = temp_ws("pro-historia");
        let store = Store::for_workspace(&ws);
        store.init().unwrap();
        store
            .append_transcript(
                "stara-sesja",
                Some(2),
                "tool_result",
                "{\"kind\":\"tool_result\",\"data\":{\"output\":\"raport kwartalny gotowy\"}}",
            )
            .unwrap();

        let section = history_section(&ws, "pokaz raport");
        assert!(
            section.contains("TRAFIENIA Z HISTORII"),
            "sekcja historii: {section}"
        );
        assert!(section.contains("stara-sesja"), "skad trafienie: {section}");
        assert!(history_section(&ws, ":: ;").is_empty(), "puste zapytanie");
        let _ = fs::remove_dir_all(&ws);
    }

    // ── Runda 15: tryb `ask` ───────────────────────────────────────────────

    /// Odmowa użytkownika: efekt nie powstaje, transkrypt ma `USER_DENIED`,
    /// licznik odmów rośnie, a sesja nie kończy się `FAILED_STALLED`.
    #[test]
    fn ask_mode_user_denial_blocks_the_effect() {
        let ws = temp_ws("ask-deny");
        let (approver, handle) = Approver::channel();
        let asked = std::thread::spawn(move || {
            let mut seen = Vec::new();
            while let Some(request) = handle.recv() {
                seen.push(request.clone());
                // `n`: odmowa. Jedna prośba wystarcza — dalej mock kończy.
                handle.answer(false);
                if seen.len() >= 1 {
                    break;
                }
            }
            seen
        });
        let mut cfg = config(&ws, false);
        cfg.approver = Some(approver);
        let report = run("write nota.txt :: tresc", &cfg);
        let seen = asked.join().unwrap();

        assert_eq!(seen.len(), 1, "jedna prosba o zgode na fs.write");
        assert_eq!(seen[0].tool, "fs.write");
        assert_eq!(seen[0].arg, "nota.txt");
        assert!(
            !ws.join("nota.txt").exists(),
            "odmowa nie moze utworzyc pliku"
        );
        assert_eq!(report.denied, 1, "licznik odmow rosnie");
        assert_eq!(report.tool_calls, 0, "narzedzie nie poszlo");
        assert_eq!(report.status, "DONE", "odmowa nie zabija sesji: {}", report.reason);
        assert!(
            transcript_has(&report.session, "USER_DENIED"),
            "transkrypt musi miec powod USER_DENIED"
        );
        assert!(
            events_of(&report.session)
                .iter()
                .any(|e| e["kind"] == json!("denied") && e["data"]["reason"] == json!("USER_DENIED")),
            "zdarzenie `denied` z reason USER_DENIED"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    /// Zgoda: ten sam przebieg, plik powstaje, licznik odmów zostaje na zerze.
    #[test]
    fn ask_mode_user_approval_runs_the_tool() {
        let ws = temp_ws("ask-allow");
        let (approver, handle) = Approver::channel();
        let asked = std::thread::spawn(move || {
            let request = handle.recv();
            handle.answer(true);
            request
        });
        let mut cfg = config(&ws, false);
        cfg.approver = Some(approver);
        let report = run("write nota.txt :: tresc", &cfg);
        let request = asked.join().unwrap().expect("prosba o zgode");

        assert_eq!(request.tool, "fs.write");
        assert_eq!(
            fs::read_to_string(ws.join("nota.txt")).unwrap(),
            "tresc",
            "zgoda wykonuje efekt"
        );
        assert_eq!(report.denied, 0, "zgoda to nie odmowa");
        assert_eq!(report.tool_calls, 1);
        assert_eq!(report.status, "DONE", "{}", report.reason);
        assert!(!transcript_has(&report.session, "USER_DENIED"));
        let _ = fs::remove_dir_all(&ws);
    }

    /// `fs.read` nie ma efektu ubocznego — w trybie `ask` nie pyta o zgodę.
    #[test]
    fn ask_mode_does_not_ask_about_read_only_tools() {
        let ws = temp_ws("ask-read");
        fs::write(ws.join("a.txt"), "alfa").unwrap();
        let (approver, handle) = Approver::channel();
        let mut cfg = config(&ws, false);
        cfg.approver = Some(approver);
        // Bez odpowiedzi: gdyby agent zapytał o `fs.read`, `run` stanąłby na
        // bramce i test nie doszedłby do asercji.
        let report = run("read a.txt", &cfg);
        assert!(handle.try_recv().is_none(), "fs.read nie pyta o zgode");
        assert_eq!(report.denied, 0);
        assert_eq!(report.tool_calls, 1);
        assert_eq!(report.status, "DONE", "{}", report.reason);
        let _ = fs::remove_dir_all(&ws);
    }

}
