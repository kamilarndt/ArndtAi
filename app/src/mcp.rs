//! Perimeter MCP po stdio (JSON-RPC 2.0). Kontrakt: docs/contracts/mcp.md v0.1.
//!
//! Serwer: `ardntai mcp --workspace FOLDER` — życie: initialize (handshake
//! wersji kontraktu) -> tools/list -> tools/call -> shutdown.
//! Klient: uruchamia własny binarny plik jako osobny proces, więc narzędzia
//! nie dzielą pamięci z pętlą agenta.

use crate::tools;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

pub const PROTOCOL_VERSION: &str = "2024-11-05";
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

// ---------------------------------------------------------------------------
// Serwer
// ---------------------------------------------------------------------------

pub fn serve(workspace: PathBuf) -> i32 {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    json!({
                        "jsonrpc": "2.0",
                        "id": Value::Null,
                        "error": { "code": -32700, "message": format!("niepoprawny JSON: {}", e) }
                    })
                );
                let _ = stdout.flush();
                continue;
            }
        };

        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));

        let (result, error, stop) = handle(&method, &params, &workspace);

        let message = match (id, error) {
            (Some(id), Some((code, text))) => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": code, "message": text }
            }),
            (Some(id), None) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            (None, _) => continue, // powiadomienie — bez odpowiedzi
        };

        if writeln!(stdout, "{}", message).is_err() {
            break;
        }
        let _ = stdout.flush();
        if stop {
            break;
        }
    }
    0
}

fn handle(method: &str, params: &Value, workspace: &Path) -> (Value, Option<(i64, String)>, bool) {
    match method {
        "initialize" => {
            let requested = params
                .get("protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or(PROTOCOL_VERSION);
            (
                json!({
                    "protocolVersion": requested,
                    "contractVersion": tools::CONTRACT_VERSION,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "ardntai", "version": env!("CARGO_PKG_VERSION") }
                }),
                None,
                false,
            )
        }
        "notifications/initialized" => (Value::Null, None, false),
        "ping" => (json!({}), None, false),
        "tools/list" => (json!({ "tools": tools::list_tools() }), None, false),
        "tools/call" => (call_tool(params, workspace), None, false),
        "shutdown" => (Value::Null, None, true),
        other => (
            Value::Null,
            Some((-32601, format!("nieznana metoda: {}", other))),
            false,
        ),
    }
}


/// Odtworzenie wyniku z zapisanego kroku (`steps.result_json`).
fn tool_result_from_json(stored: &Value) -> tools::ToolResult {
    let ok = stored.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
    let code = stored
        .get("code")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let bytes = stored.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let output = stored.get("output").cloned().unwrap_or_else(|| json!({}));
    tools::ToolResult {
        ok,
        code,
        output,
        bytes,
    }
}

fn call_tool(params: &Value, workspace: &Path) -> Value {
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let task_id = params.get("task_id").and_then(|v| v.as_str()).unwrap_or("");
    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let step_id = params.get("step_id").and_then(|v| v.as_u64());

    let result = if name.is_empty() {
        tools::ToolResult::error("SCHEMA_ERROR", "brak nazwy narzedzia")
    } else if task_id.is_empty() || session_id.is_empty() || step_id.is_none() {
        tools::ToolResult::error(
            "SCHEMA_ERROR",
            "wywolanie musi zawierac task_id, session_id i step_id",
        )
    } else {
        // F3: retry po tym samym `step_id` nie moze wykonac efektu drugi raz.
        // Ten sam gate co w petli agenta, na tej samej bazie `.agent/state.db`.
        let step = step_id.unwrap_or(0);
        let store = crate::store::Store::for_workspace(workspace);
        let gate = store.init().and_then(|_| {
            store.create_session(session_id, "mcp")?;
            store.fetch_step(session_id, step)
        });
        let args_json = args.to_string();
        match gate {
            Ok(Some(row)) if row.status == "done" => {
                if row.tool != name || row.args_json != args_json {
                    // Ten sam `step_id` z innymi argumentami to blad klienta:
                    // wykonanie daloby drugi, nieidempotentny efekt.
                    tools::ToolResult::error(
                        "DENIED",
                        format!(
                            "step_id {} jest juz uzyty z innymi argumentami (tool={})",
                            step, row.tool
                        ),
                    )
                } else {
                    let stored: Value = row
                        .result_json
                        .and_then(|raw| serde_json::from_str(&raw).ok())
                        .unwrap_or_else(|| json!({ "ok": true, "output": {}, "bytes": 0 }));
                    tool_result_from_json(&stored)
                }
            }
            Ok(Some(row)) if row.status == "proposed" || row.status == "abandoned" => {
                let note = json!({ "skipped": "crash_before_result" }).to_string();
                if row.status == "proposed" {
                    let _ = store.mark_abandoned(session_id, step, &note);
                }
                tools::ToolResult::error(
                    "SKIPPED",
                    "krok przerwany przed wynikiem — nie powtarzam (at-most-once)",
                )
            }
            _ => {
                let _ = store.proposed(session_id, step, name, &args_json);
                let r = tools::call(name, &args, workspace);
                let result_json = json!({
                    "ok": r.ok,
                    "code": r.code,
                    "bytes": r.bytes,
                    "output": r.output,
                })
                .to_string();
                let persisted = if name == "notify.send" && r.ok {
                    store.complete_with_outbox(session_id, step, &result_json, &args_json)
                } else {
                    store.complete(session_id, step, &result_json)
                };
                let _ = persisted;
                r
            }
        }
    };

    let text = result.output.to_string();
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": !result.ok,
        "structuredContent": result.output
    })
}

// ---------------------------------------------------------------------------
// Klient
// ---------------------------------------------------------------------------

/// Blad warstwy transportu MCP. Kazdy wariant ma kod, ktory trafia do
/// `ToolResult::error` — blad serwera nie moze zamykac petli panika.
#[derive(Debug, Clone)]
pub enum McpError {
    /// Brak odpowiedzi w oknie `CALL_TIMEOUT`.
    Timeout(Duration),
    /// Linia odpowiedzi nie jest poprawnym JSON-em.
    InvalidJson(String),
    /// Poprawna koperta JSON-RPC z polem `error`.
    Rpc { code: i64, message: String },
    /// Awaria zapisu/czytania po naszej stronie (stdin zamkniety itp.).
    Transport(String),
}

impl McpError {
    /// Kod kontraktu (`docs/contracts/mcp.md`); kod JSON-RPC jest zachowany.
    pub fn code(&self) -> String {
        match self {
            McpError::Timeout(_) => "TIMEOUT".to_string(),
            McpError::InvalidJson(_) | McpError::Transport(_) => "IO_ERROR".to_string(),
            McpError::Rpc { code, .. } => code.to_string(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            McpError::Timeout(d) => format!(
                "TIMEOUT: serwer MCP nie odpowiedzial w {} s",
                d.as_secs_f32()
            ),
            McpError::InvalidJson(e) => format!("niepoprawna odpowiedz MCP: {}", e),
            McpError::Rpc { code, message } => format!("blad JSON-RPC {}: {}", code, message),
            McpError::Transport(e) => e.clone(),
        }
    }

    /// Jedyna droga wyjscia bledu na zewnatrz klienta.
    pub fn into_tool_result(self) -> tools::ToolResult {
        tools::ToolResult::error(&self.code(), self.message())
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

/// Odpowiedz serwera: JSON-RPC z polem `error` to blad, nie wynik.
fn parse_response(line: &str) -> Result<Value, McpError> {
    let response: Value =
        serde_json::from_str(line.trim()).map_err(|e| McpError::InvalidJson(e.to_string()))?;
    if let Some(err) = response.get("error") {
        let code = err.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        let message = err.get("message").and_then(|v| v.as_str()).unwrap_or("");
        return Err(McpError::Rpc {
            code,
            message: message.to_string(),
        });
    }
    Ok(response)
}

/// Czeka na odpowiedz nie dluzej niz `timeout`.
fn wait_response(rx: &Receiver<String>, timeout: Duration) -> Result<Value, McpError> {
    let line = rx
        .recv_timeout(timeout)
        .map_err(|_| McpError::Timeout(timeout))?;
    parse_response(&line)
}

pub struct McpClient {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<String>,
    next_id: u64,
}

// ---------------------------------------------------------------------------
// Wlasna sciezka (runda 13)
// ---------------------------------------------------------------------------

/// Linux dopisuje `" (deleted)"` do sciezki z `/proc/self/exe`, gdy plik
/// binarki zostal podmieniony (albo usuniety) w trakcie dzialania procesu.
/// To nie jest prawdziwa sciezka — `Command::new` dostalby nieistniejacy plik.
pub fn strip_deleted_suffix(path: &str) -> &str {
    path.strip_suffix(" (deleted)").unwrap_or(path)
}

/// Wybiera sciezke do wlasnej binarki z podanych kandydatow.
///
/// Kolejnosc: `current_exe` (po odcieciu `" (deleted)"`) -> `ARNDTAI_SELF` ->
/// `ardntai` w katalogach z `PATH`. Pierwszy istniejący plik wygrywa. Blad
/// niesie liste sprawdzonych sciezek i zdanie wyjasnienia, bo bez tego komunikat
/// „No such file or directory" nic nie mowi.
pub fn resolve_from(
    exe: Option<&str>,
    self_env: Option<&str>,
    path_dirs: &[PathBuf],
) -> Result<PathBuf, String> {
    let mut tried: Vec<String> = Vec::new();
    let mut replaced = false;

    match exe {
        Some(raw) => {
            let stripped = strip_deleted_suffix(raw);
            replaced = stripped != raw;
            let candidate = PathBuf::from(stripped);
            tried.push(candidate.display().to_string());
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        None => tried.push("current_exe: nieznana".to_string()),
    }

    match self_env.map(str::trim).filter(|v| !v.is_empty()) {
        Some(value) => {
            let candidate = PathBuf::from(value);
            tried.push(format!("{} (ARNDTAI_SELF)", candidate.display()));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        None => tried.push("ARNDTAI_SELF: nie ustawiona".to_string()),
    }

    for dir in path_dirs {
        let candidate = dir.join("ardntai");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    tried.push("PATH: brak ardtai".to_string());

    let hint = if replaced {
        "; binarka zostala podmieniona w trakcie dzialania procesu"
    } else {
        ""
    };
    Err(format!(
        "nie mozna ustalic wlasnej sciezki{}; sprawdzilem: {}",
        hint,
        tried.join(", ")
    ))
}

/// Sciezka do siebie wzięta z prawdziwego srodowiska (patrz [`resolve_from`]).
pub fn resolve_self_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_string());
    let self_env = std::env::var("ARNDTAI_SELF").ok();
    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    resolve_from(exe.as_deref(), self_env.as_deref(), &path_dirs)
}

impl McpClient {
    pub fn spawn(workspace: &Path) -> Result<McpClient, String> {
        let exe = resolve_self_exe()?;
        let mut child = Command::new(&exe)
            .arg("mcp")
            .arg("--workspace")
            .arg(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                // `NotFound` po udanym `is_file()` znaczy, że plik znikł między
                // sprawdzeniem a `exec` — czyli podmiana binarki w trakcie pracy.
                let hint = if e.kind() == std::io::ErrorKind::NotFound {
                    "; binarka zostala podmieniona albo usunieta w trakcie dzialania procesu — ustaw ARNDTAI_SELF na nowa sciezke"
                } else {
                    ""
                };
                format!(
                    "nie mozna uruchomic serwera MCP ({}): {}{}",
                    exe.display(),
                    e,
                    hint
                )
            })?;

        let stdin = child.stdin.take().ok_or("serwer MCP nie ma stdin")?;
        let stdout: ChildStdout = child.stdout.take().ok_or("serwer MCP nie ma stdout")?;

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let mut client = McpClient {
            child,
            stdin: Some(stdin),
            rx,
            next_id: 0,
        };

        let info = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "clientInfo": { "name": "ardntai-agent", "version": env!("CARGO_PKG_VERSION") }
                }),
            )
            .map_err(|e| e.to_string())?;
        let version = info
            .get("contractVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if version != tools::CONTRACT_VERSION {
            return Err(format!(
                "serwer MCP zglasza kontrakt '{}', oczekiwany '{}'",
                version,
                tools::CONTRACT_VERSION
            ));
        }
        Ok(client)
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        self.next_id += 1;
        let message = json!({
            "jsonrpc": "2.0",
            "id": self.next_id,
            "method": method,
            "params": params
        });
        {
            let stdin = self
                .stdin
                .as_mut()
                .ok_or_else(|| McpError::Transport("stdin serwera MCP zamkniety".to_string()))?;
            writeln!(stdin, "{}", message)
                .map_err(|e| McpError::Transport(format!("zapis do serwera MCP: {}", e)))?;
            stdin
                .flush()
                .map_err(|e| McpError::Transport(format!("flush do serwera MCP: {}", e)))?;
        }
        let response = wait_response(&self.rx, CALL_TIMEOUT)?;
        Ok(response.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn list_tools(&mut self) -> Result<Vec<String>, String> {
        let result = self
            .request("tools/list", json!({}))
            .map_err(|e| e.to_string())?;
        Ok(result
            .get("tools")
            .and_then(|v| v.as_array())
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn call(
        &mut self,
        name: &str,
        args: &Value,
        ctx: &tools::CallCtx,
    ) -> Result<tools::ToolResult, String> {
        let result = match self.request(
            "tools/call",
            json!({
                "name": name,
                "arguments": args,
                "task_id": ctx.task_id,
                "session_id": ctx.session_id,
                "step_id": ctx.step_id
            }),
        ) {
            Ok(result) => result,
            // Bledy transportu (JSON-RPC, nie-JSON, timeout) wracaja jako
            // `ToolResult::error` z kodem, tak samo jak bledy narzedzi.
            Err(e) => return Ok(e.into_tool_result()),
        };
        let is_error = result
            .get("isError")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let output = result
            .get("structuredContent")
            .cloned()
            .unwrap_or(Value::Null);
        let code = output
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let bytes = bytes_from_output(&output);
        Ok(tools::ToolResult {
            ok: !is_error,
            code,
            output,
            bytes,
        })
    }

    pub fn shutdown(&mut self) {
        if self.stdin.is_none() {
            return;
        }
        let _ = self.request("shutdown", json!({}));
        self.stdin = None;
        let _ = self.child.wait();
    }
}

/// Rozmiar wyniku z `structuredContent`: `size` (fs.read), `bytes_written`
/// (fs.write/fs.append) albo `bytes` (skill.load). Runda 17: brak ostatniego
/// pola pokazywal `skill.load ok=true (0 bajtow)` mimo wczytanej tresci.
fn bytes_from_output(output: &Value) -> usize {
    output
        .get("size")
        .and_then(|v| v.as_u64())
        .or_else(|| output.get("bytes_written").and_then(|v| v.as_u64()))
        .or_else(|| output.get("bytes").and_then(|v| v.as_u64()))
        .unwrap_or(0) as usize
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_ws(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-mcp-{}-{}-{}",
            tag,
            std::process::id(),
            now_stamp()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn now_stamp() -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    }

    fn params(content: &str) -> Value {
        json!({
            "name": "fs.write",
            "arguments": { "path": "x.txt", "content": content },
            "task_id": "t1",
            "session_id": "s1",
            "step_id": 1
        })
    }

    #[test]
    fn repeated_step_id_does_not_repeat_side_effect() {
        let ws = temp_ws("dedup");
        let first = call_tool(&params("pierwszy"), &ws);
        assert_eq!(first["isError"], json!(false), "{}", first);
        assert_eq!(fs::read_to_string(ws.join("x.txt")).unwrap(), "pierwszy");

        // Retry tego samego step_id: zwraca zapisany wynik, zero efektu.
        let retry = call_tool(&params("pierwszy"), &ws);
        assert_eq!(retry["isError"], json!(false), "{}", retry);
        assert_eq!(
            first["structuredContent"], retry["structuredContent"],
            "retry musi zwrocic zapisany wynik"
        );

        // Ten sam step_id z innymi argumentami: odmowa, nie drugi efekt.
        let reused = call_tool(&params("drugi"), &ws);
        assert_eq!(reused["isError"], json!(true), "{}", reused);
        assert!(reused["structuredContent"]["message"]
            .as_str()
            .unwrap()
            .contains("step_id"));
        assert_eq!(
            fs::read_to_string(ws.join("x.txt")).unwrap(),
            "pierwszy",
            "powtorka step_id nie moze nadpisac efektu"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    // F22: sciezki bledow klienta MCP. Kazda konczy sie `ToolResult::error`
    // z kodem, bez paniki — zaden blad serwera nie moze wysadzic petli.

    #[test]
    fn json_rpc_error_response_becomes_tool_error_with_code() {
        let line = r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32603,"message":"boom"}}"#;
        let err = parse_response(line).expect_err("koperta z `error` musi byc bledem");
        assert_eq!(err.code(), "-32603");

        let result = err.into_tool_result();
        assert!(!result.ok);
        assert_eq!(result.code.as_deref(), Some("-32603"));
        assert_eq!(result.output["error"], json!("-32603"));
        assert!(
            result.output["message"].as_str().unwrap().contains("boom"),
            "{}",
            result.output
        );
        assert_eq!(result.bytes, 0);
    }

    #[test]
    fn response_that_is_not_json_is_an_error_not_a_panic() {
        let err = parse_response("<html>502 Bad Gateway</html>")
            .expect_err("nie-JSON musi byc bledem, nie panika");
        assert_eq!(err.code(), "IO_ERROR");

        let result = err.into_tool_result();
        assert!(!result.ok);
        assert_eq!(result.code.as_deref(), Some("IO_ERROR"));
        assert!(
            result.output["message"]
                .as_str()
                .unwrap()
                .contains("niepoprawna odpowiedz"),
            "{}",
            result.output
        );
    }

    #[test]
    fn strip_deleted_suffix_drops_linux_marker_only() {
        assert_eq!(strip_deleted_suffix("/x/ardntai (deleted)"), "/x/ardntai");
        assert_eq!(strip_deleted_suffix("/x/ardntai"), "/x/ardntai");
        assert_eq!(
            strip_deleted_suffix("/x/ardntai (deleted) (deleted)"),
            "/x/ardntai (deleted)",
            "odcinamy jeden sufiks, nie wszystkie"
        );
        assert_eq!(strip_deleted_suffix("/x/my (deleted) app"), "/x/my (deleted) app");
    }

    /// Scenariusz z rundy 13: plik podmieniony w trakcie pracy procesu, wiec
    /// `current_exe` daje sciezke z `" (deleted)"`, a plik pod nia nie istnieje.
    /// Fallback `ARNDTAI_SELF` musi wskazac nowa binarke.
    #[test]
    fn replaced_binary_falls_back_to_self_env() {
        let dir = temp_ws("selfenv");
        let fresh = dir.join("ardntai-new");
        fs::write(&fresh, b"binarka").unwrap();

        let deleted = format!("{} (deleted)", dir.join("ardntai").display());
        let picked = resolve_from(Some(&deleted), Some(&fresh.to_string_lossy()), &[])
            .expect("ARNDTAI_SELF ma uratowac proces po podmianie binarki");
        assert_eq!(picked, fresh);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_binary_falls_back_to_path() {
        let dir = temp_ws("selfpath");
        let bin_dir = dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let in_path = bin_dir.join("ardntai");
        fs::write(&in_path, b"binarka").unwrap();

        let deleted = format!("{} (deleted)", dir.join("ardntai").display());
        let picked = resolve_from(Some(&deleted), None, &[dir.join("pusty"), bin_dir.clone()])
            .expect("PATH ma byc ostatnim fallbackiem");
        assert_eq!(picked, in_path);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn all_fallbacks_missing_reports_paths_and_reason() {
        let dir = temp_ws("selfmiss");
        let deleted = format!("{} (deleted)", dir.join("ardntai").display());
        let err = resolve_from(Some(&deleted), None, &[dir.join("pusty")])
            .expect_err("bez zadnego kandydata musi byc blad");
        assert!(
            err.contains(&dir.join("ardntai").display().to_string()),
            "komunikat musi podac sciezke, ktorej probowano: {err}"
        );
        assert!(
            err.contains("podmieniona w trakcie dzialania procesu"),
            "komunikat musi wyjasnic przyczyne: {err}"
        );
        assert!(err.contains("ARNDTAI_SELF"), "musi wymienic fallbacki: {err}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_exe_wins_over_env() {
        let dir = temp_ws("selfwin");
        let real = dir.join("ardntai");
        fs::write(&real, b"binarka").unwrap();
        let other = dir.join("inna");
        fs::write(&other, b"binarka").unwrap();

        let picked = resolve_from(
            Some(&real.to_string_lossy()),
            Some(&other.to_string_lossy()),
            &[],
        )
        .expect("istniejacy current_exe ma wygrac");
        assert_eq!(picked, real);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn silent_server_is_a_timeout_error_not_a_panic() {
        // Kanal bez nadawcy danych: `wait_response` czeka do konca timeoutu.
        let (_tx, rx) = mpsc::channel::<String>();
        let started = std::time::Instant::now();
        let err = wait_response(&rx, Duration::from_millis(80))
            .expect_err("brak odpowiedzi musi byc timeoutem");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "timeout musi byc ograniczony parametrem, nie stalym 30 s"
        );
        assert_eq!(err.code(), "TIMEOUT");

        let result = err.into_tool_result();
        assert!(!result.ok);
        assert_eq!(result.code.as_deref(), Some("TIMEOUT"));
    }

    #[test]
    fn call_without_envelope_is_rejected() {
        let ws = temp_ws("envelope");
        let mut p = params("x");
        p.as_object_mut().unwrap().remove("step_id");
        let out = call_tool(&p, &ws);
        assert_eq!(out["isError"], json!(true));
        assert!(out["structuredContent"]["message"]
            .as_str()
            .unwrap()
            .contains("step_id"));
        let _ = fs::remove_dir_all(&ws);
    }

    /// Runda 17: `skill.load` raportuje rozmiar polem `bytes`, nie `size`
    /// ani `bytes_written` — klient MCP musi je przeczytac z wyniku.
    #[test]
    fn skill_load_bytes_are_read_from_the_result_envelope() {
        assert_eq!(
            bytes_from_output(&json!({ "content": "x", "bytes": 331, "truncated": false })),
            331
        );
        assert_eq!(bytes_from_output(&json!({ "bytes": 0 })), 0);
        assert_eq!(bytes_from_output(&json!({ "size": 12 })), 12);
        assert_eq!(bytes_from_output(&json!({ "bytes_written": 7 })), 7);
        assert_eq!(bytes_from_output(&json!({})), 0);
    }
}
