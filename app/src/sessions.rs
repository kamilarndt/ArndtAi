//! Ekran startowy: lista sesji czytana z prawdziwych transkryptów.
//!
//! Model wiersza jest przeniesiony z dashboardu pagera Grok Build
//! (`views/dashboard/row.rs`, `row_title.rs`, `row_activity.rs`, `state.rs`):
//! tytuł, przygaszony ` · subtitle`, stan, druga linia z ostatnią aktywnością,
//! badge, wiek wpisu i skrócona ścieżka. Dane biorą się wyłącznie z
//! `.agent/sessions/*.jsonl` i `.agent/resume` — żadnego wiersza nie ma tu
//! wymyślonego; czego transkrypt nie zapisuje (procent kontekstu, gałąź per
//! sesja), po prostu nie ma.
//!
//! Kontrakt eventów (`docs/contracts/events.md`) zostaje bez zmian: to ten sam
//! JSONL, tylko czytany, nie zapisywany.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Długość podglądu tytułu — `row.rs`: `chars().take(30)` dla pierwszego zadania.
pub const TITLE_LIMIT: usize = 30;

/// Sesja bez zadania (np. przerwana przed pierwszym eventem) dostaje etykietę
/// jak `NEW_SESSION_LABEL` w oryginale, z przygaszonym ` #<id>`.
pub const NEW_SESSION_LABEL: &str = "Nowa sesja";

/// Transkrypt bez podsumowania, który stanął na wywołaniu narzędzia albo na
/// odmowie, czeka na decyzję użytkownika — tyle wystarczy, by nazwać go
/// `needs-input`. Po tym czasie bez podsumowania uznajemy sesję za bezczynną.
pub const RUNNING_WINDOW_SECS: u64 = 300;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Ostatni event to `tool_call`/`denied` bez wyniku — trzeba zdecydować.
    NeedsInput,
    /// Transkrypt bez podsumowania, świeży — tura leci albo właśnie padła.
    Running,
    /// Bez podsumowania i bez ruchu od `RUNNING_WINDOW_SECS`.
    Idle,
    Done,
    Failed,
}

impl State {
    /// Kolejność grup z `RowState::group_priority` (oryginał: 6..0).
    pub fn group_priority(self) -> u8 {
        match self {
            State::NeedsInput => 6,
            State::Running => 5,
            State::Idle => 3,
            State::Done => 1,
            State::Failed => 0,
        }
    }

    /// Nagłówek grupy — `RowState::group_label` z oryginału.
    pub fn group_label(self) -> &'static str {
        match self {
            State::NeedsInput => "Czeka na Ciebie",
            State::Running => "W toku",
            State::Idle => "Bezczynna",
            State::Done => "Gotowe",
            State::Failed => "Blad",
        }
    }

}

/// Badge wiersza — `RowBadge` z oryginału, ograniczony do tego, co ArndtAi
/// naprawdę wie: `worktree` (workspace jest linked worktree), `needs-input`,
/// `failed`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Badge {
    Worktree,
    NeedsInput,
    Failed,
}

impl Badge {
    pub fn label(self) -> &'static str {
        match self {
            Badge::Worktree => "worktree",
            Badge::NeedsInput => "needs-input",
            Badge::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub state: State,
    /// Druga linia: ostatnie narzędzie albo ostatnia myśl (`row_activity.rs`).
    pub activity: Option<String>,
    pub badges: Vec<Badge>,
    /// Ścieżka skrócona jak `display_location_path` (`~/…`).
    pub cwd: String,
    /// Znacznik czasu ostatniego eventu (sekundy uniksowe).
    pub last_change: u64,
    pub steps: u64,
    pub tool_calls: u64,
    pub denied: u64,
}

impl SessionRow {
    pub fn age_secs(&self, now: u64) -> u64 {
        now.saturating_sub(self.last_change)
    }

    pub fn badges_text(&self) -> String {
        self.badges
            .iter()
            .map(|b| b.label())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Filtr listy — odpowiednik `Filter::State` z dashboardu, zawężony do trzech
/// stanów, które użytkownik chce przełączać jednym klawiszem.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Filter {
    All,
    Failed,
    Running,
}

impl Filter {
    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "wszystkie",
            Filter::Failed => "bledy",
            Filter::Running => "w toku",
        }
    }

    /// Kolejność z klawisza `/`: wszystkie → bledy → w toku → wszystkie.
    pub fn next(self) -> Filter {
        match self {
            Filter::All => Filter::Failed,
            Filter::Failed => Filter::Running,
            Filter::Running => Filter::All,
        }
    }

    pub fn matches(self, row: &SessionRow) -> bool {
        match self {
            Filter::All => true,
            Filter::Failed => row.state == State::Failed,
            Filter::Running => matches!(row.state, State::Running | State::NeedsInput),
        }
    }
}

/// Sesje workspace'u, od najbardziej pilnej grupy (`group_priority`) i od
/// najświeższej w grupie — sortowanie z `sort_row_key`.
pub fn scan(workspace: &Path) -> Vec<SessionRow> {
    scan_at(workspace, now_secs())
}

pub fn scan_at(workspace: &Path, now: u64) -> Vec<SessionRow> {
    let home = std::env::var("HOME").ok();
    let branch = branch_label(workspace);
    let worktree = is_linked_worktree(workspace);
    let mut rows: Vec<SessionRow> = Vec::new();
    for path in transcripts(workspace) {
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
            continue;
        };
        let events = read_events(&path);
        if events.is_empty() {
            continue;
        }
        if let Some(row) = build_row(&id, &events, now, workspace, &home, &branch, worktree) {
            rows.push(row);
        }
    }
    sort_rows(&mut rows);
    rows
}

/// Pliki transkryptów, od najstarszego (jak `context::sessions`).
pub fn transcripts(workspace: &Path) -> Vec<PathBuf> {
    crate::context::sessions(workspace)
}

/// Eventy jednego transkryptu; linie nie-JSON są pomijane, nie wysadzają listy.
pub fn read_events(path: &Path) -> Vec<Value> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .collect()
}

/// Transkrypt po `id` (wejście w sesję z listy).
pub fn transcript(workspace: &Path, id: &str) -> Vec<Value> {
    read_events(
        &workspace
            .join(".agent")
            .join("sessions")
            .join(format!("{}.jsonl", id)),
    )
}

fn build_row(
    id: &str,
    events: &[Value],
    now: u64,
    workspace: &Path,
    home: &Option<String>,
    branch: &Option<String>,
    worktree: bool,
) -> Option<SessionRow> {
    let last = events.last()?;
    let last_kind = kind(last);
    let last_at = last.get("at").and_then(|v| v.as_u64()).unwrap_or(now);

    let title = first_task(events)
        .map(|text| text.chars().take(TITLE_LIMIT).collect::<String>())
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| format!("{} #{}", NEW_SESSION_LABEL, short_id(id)));

    let state = classify(last_kind, last, now.saturating_sub(last_at));
    let (steps, tool_calls, denied) = counters(events);

    let mut badges = Vec::new();
    if worktree {
        badges.push(Badge::Worktree);
    }
    match state {
        State::NeedsInput => badges.push(Badge::NeedsInput),
        State::Failed => badges.push(Badge::Failed),
        _ => {}
    }

    Some(SessionRow {
        id: id.to_string(),
        title,
        subtitle: branch.clone(),
        state,
        activity: activity(events),
        badges,
        cwd: shorten_location(&workspace.display().to_string(), home.as_deref()),
        last_change: last_at,
        steps,
        tool_calls,
        denied,
    })
}

fn classify(last_kind: &str, last: &Value, age: u64) -> State {
    match last_kind {
        "summary" => {
            let status = last
                .get("data")
                .and_then(|d| d.get("status"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if status == "DONE" {
                State::Done
            } else {
                State::Failed
            }
        }
        "error" | "store_error" => State::Failed,
        "tool_call" | "denied" => State::NeedsInput,
        _ => {
            if age <= RUNNING_WINDOW_SECS {
                State::Running
            } else {
                State::Idle
            }
        }
    }
}

fn counters(events: &[Value]) -> (u64, u64, u64) {
    let summary = events
        .iter()
        .rev()
        .find(|e| kind(e) == "summary")
        .and_then(|e| e.get("data"));
    match summary {
        Some(data) => (
            num(data, "steps"),
            num(data, "tool_calls"),
            num(data, "denied"),
        ),
        None => {
            let tools = events
                .iter()
                .filter(|e| {
                    matches!(
                        kind(e),
                        "tool_result" | "tool_result_cached" | "tool_result_skipped"
                    )
                })
                .count() as u64;
            let denied = events.iter().filter(|e| kind(e) == "denied").count() as u64;
            let steps = events
                .iter()
                .filter(|e| kind(e) == "thought")
                .count() as u64;
            (steps, tools, denied)
        }
    }
}

/// Ostatnia aktywność: narzędzie (czas teraźniejszy w toku, przeszły po
/// wyniku), odmowa, myśl albo ostatnie podsumowanie — `row_activity.rs`.
fn activity(events: &[Value]) -> Option<String> {
    for (index, event) in events.iter().enumerate().rev() {
        let data = match event.get("data") {
            Some(d) => d,
            None => continue,
        };
        let text = match kind(event) {
            "tool_result" | "tool_result_cached" | "tool_result_skipped" => {
                let tool = str_of(data, "tool");
                let step = num(data, "step");
                let args = data
                    .get("args")
                    .cloned()
                    .or_else(|| call_args(&events[..index], step, &tool));
                let subject = tool_subject(&tool, args.as_ref(), data.get("output"));
                let ok = data.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
                let mark = if ok { "" } else { "\u{2717} " };
                Some(format!("{}{} {}", mark, past_verb(&tool), subject))
            }
            "tool_call" => {
                let tool = str_of(data, "tool");
                let subject = tool_subject(&tool, data.get("args"), None);
                Some(format!("{} {}", present_verb(&tool), subject))
            }
            "denied" => {
                let tool = str_of(data, "tool");
                let reason = str_of(data, "reason");
                Some(format!("Pending: {} — {}", tool, reason))
            }
            "thought" => {
                let thought = str_of(data, "thought");
                first_line(&thought).map(|line| format!("Thought: {}", line))
            }
            "summary" => {
                let reason = str_of(data, "reason");
                let status = str_of(data, "status");
                if reason.trim().is_empty() {
                    None
                } else {
                    Some(format!("{} — {}", status, reason))
                }
            }
            "task" => first_line(&str_of(data, "text")).map(|line| format!("Zadanie: {}", line)),
            _ => None,
        };
        if let Some(text) = text {
            return Some(short(&text, 120));
        }
    }
    None
}

/// Argumenty wywołania, którego dotyczy wynik: `tool_result` nie powtarza
/// `args`, więc szukamy `tool_call` o tym samym kroku i narzędziu wcześniej
/// w transkrypcie (to samo wiązanie, którego używa TUI: krok + narzędzie).
fn call_args(events: &[Value], step: u64, tool: &str) -> Option<Value> {
    events.iter().rev().find_map(|event| {
        if kind(event) != "tool_call" {
            return None;
        }
        let data = event.get("data")?;
        if num(data, "step") != step {
            return None;
        }
        let called = str_of(data, "tool");
        if !tool.is_empty() && called != tool {
            return None;
        }
        data.get("args").cloned()
    })
}

fn first_task(events: &[Value]) -> Option<String> {
    events.iter().find(|e| kind(e) == "task").and_then(|e| {
        e.get("data")
            .and_then(|d| d.get("text"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    })
}

fn tool_subject(tool: &str, args: Option<&Value>, output: Option<&Value>) -> String {
    let args = args.cloned().unwrap_or(Value::Null);
    match tool {
        "fs.read" | "fs.write" | "fs.append" => {
            let path = str_of(&args, "path");
            if path.is_empty() {
                tool.to_string()
            } else {
                path
            }
        }
        "notify.send" => {
            let message = str_of(&args, "message");
            if message.is_empty() {
                tool.to_string()
            } else {
                message
            }
        }
        _ => match output {
            Some(out) => short(&compact(out), 40),
            None => tool.to_string(),
        },
    }
}

pub fn past_verb(tool: &str) -> &'static str {
    match tool {
        "fs.read" => "Read",
        "fs.write" => "Wrote",
        "fs.append" => "Appended",
        "notify.send" => "Sent",
        _ => "Ran",
    }
}

pub fn present_verb(tool: &str) -> &'static str {
    match tool {
        "fs.read" => "Reading",
        "fs.write" => "Writing",
        "fs.append" => "Appending",
        "notify.send" => "Sending",
        _ => "Running",
    }
}

/// Sortowanie z `sort_row_key`: grupa (priority malejąco), potem najświeższy,
/// potem `id` — bez zależności od kolejności z read_dir.
pub fn sort_rows(rows: &mut [SessionRow]) {
    rows.sort_by(|a, b| {
        b.state
            .group_priority()
            .cmp(&a.state.group_priority())
            .then_with(|| b.last_change.cmp(&a.last_change))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// `index + 2 >= len` trzyma ostatnie dwa komponenty w całości, reszta jest
/// skracana do pierwszej litery — `shorten_location_path` z oryginału.
pub fn shorten_location(path: &str, home: Option<&str>) -> String {
    let abbreviated = match home {
        Some(home) if !home.is_empty() => match path.strip_prefix(home) {
            Some("") => "~".to_string(),
            Some(rest) => format!("~/{}", rest.trim_start_matches('/')),
            None => path.to_string(),
        },
        _ => path.to_string(),
    };
    let (prefix, rest) = if abbreviated == "~" {
        ("~", "")
    } else if let Some(rest) = abbreviated.strip_prefix("~/") {
        ("~", rest)
    } else if abbreviated.starts_with('/') {
        ("", abbreviated.trim_start_matches('/'))
    } else {
        ("", abbreviated.as_str())
    };
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    if parts.len() <= 2 {
        return abbreviated;
    }
    let mut out = String::new();
    out.push_str(prefix);
    for (i, part) in parts.iter().enumerate() {
        if i + 2 >= parts.len() {
            out.push('/');
            out.push_str(part);
        } else {
            out.push('/');
            out.push_str(&shorten_component(part));
        }
    }
    out
}

fn shorten_component(part: &str) -> String {
    let mut end = 0;
    for (i, c) in part.char_indices() {
        end = i + c.len_utf8();
        if c != '.' {
            break;
        }
    }
    part[..end].to_string()
}

/// Wiek wpisu — progi z `format_time_ago` (`just now`, `5m`, `3h`, `2d`…),
/// po polsku tylko pierwszy człon.
pub fn format_age(secs: u64) -> String {
    if secs < 60 {
        return "teraz".to_string();
    }
    if secs < 3600 {
        return format!("{}m", secs / 60);
    }
    if secs < 86_400 {
        return format!("{}h", secs / 3600);
    }
    let days = secs / 86_400;
    if days < 30 {
        return format!("{}d", days);
    }
    if days < 365 {
        return format!("{}mo", days / 30);
    }
    format!("{}y", days / 365)
}

fn branch_label(workspace: &Path) -> Option<String> {
    let head = read_head(workspace)?;
    let branch = head.strip_prefix("ref: refs/heads/")?.trim().to_string();
    if branch.is_empty() {
        Some("detached".to_string())
    } else {
        Some(branch)
    }
}

/// `git worktree` zostawia w korzeniu plik `.git` (`gitdir: …/worktrees/…`)
/// zamiast katalogu — to jedyny sygnał worktree, jaki ArndtAi ma bez gita.
pub fn is_linked_worktree(workspace: &Path) -> bool {
    workspace.join(".git").is_file()
}

fn read_head(workspace: &Path) -> Option<String> {
    let dot_git = workspace.join(".git");
    if dot_git.is_file() {
        let pointer = std::fs::read_to_string(&dot_git).ok()?;
        let gitdir = pointer.trim().strip_prefix("gitdir:")?.trim();
        let gitdir = if Path::new(gitdir).is_absolute() {
            PathBuf::from(gitdir)
        } else {
            workspace.join(gitdir)
        };
        return std::fs::read_to_string(gitdir.join("HEAD")).ok();
    }
    std::fs::read_to_string(dot_git.join("HEAD")).ok()
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn kind(event: &Value) -> &str {
    event.get("kind").and_then(|v| v.as_str()).unwrap_or("")
}

fn str_of(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn num(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn short(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let clipped: String = text.chars().take(limit).collect();
    format!("{}…", clipped)
}

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_ws(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "arndtai-sessions-{}-{}-{}",
            tag,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(dir.join(".agent/sessions")).unwrap();
        dir
    }

    fn write_session(ws: &Path, id: &str, lines: &[Value]) {
        let body: String = lines
            .iter()
            .map(|v| format!("{}\n", v))
            .collect::<Vec<_>>()
            .join("");
        std::fs::write(ws.join(".agent/sessions").join(format!("{}.jsonl", id)), body).unwrap();
    }

    fn event(at: u64, kind: &str, data: Value) -> Value {
        json!({ "at": at, "kind": kind, "data": data })
    }

    #[test]
    fn scan_builds_rows_from_real_transcripts() {
        let ws = temp_ws("scan");
        write_session(
            &ws,
            "100",
            &[
                event(100, "task", json!({"text": "write a.txt :: alfa; read a.txt"})),
                event(101, "thought", json!({"step": 1, "thought": "krok 1"})),
                event(102, "tool_call", json!({"step": 1, "tool": "fs.write", "args": {"path": "a.txt"}})),
                event(103, "tool_result", json!({"step": 1, "tool": "fs.write", "ok": true, "bytes": 4})),
                event(104, "summary", json!({"status": "DONE", "steps": 1, "tool_calls": 1, "denied": 0, "reason": "gotowe"})),
            ],
        );
        write_session(
            &ws,
            "200",
            &[
                event(200, "task", json!({"text": "read nie_ma.txt"})),
                event(201, "error", json!({"message": "brak pliku"})),
                event(202, "summary", json!({"status": "FAILED_ERROR", "steps": 1, "tool_calls": 1, "denied": 0})),
            ],
        );
        let rows = scan_at(&ws, 300);
        assert_eq!(rows.len(), 2, "dwa transkrypty, dwa wiersze");
        let done = rows.iter().find(|r| r.id == "100").unwrap();
        let failed = rows.iter().find(|r| r.id == "200").unwrap();
        assert_eq!(done.state, State::Done);
        // `row.rs`: tytuł to pierwsze 30 znaków zadania.
        assert_eq!(done.title, "write a.txt :: alfa; read a.tx");
        assert_eq!(done.steps, 1);
        assert_eq!(done.tool_calls, 1);
        assert_eq!(
            done.activity.as_deref(),
            Some("DONE — gotowe"),
            "druga linia to ostatnia aktywność"
        );
        assert!(done.badges.is_empty(), "bez badge'y bez powodu");
        assert_eq!(failed.state, State::Failed);
        assert_eq!(failed.badges, vec![Badge::Failed]);
        // Grupa: Done (1) przed Failed (0).
        assert_eq!(rows[0].id, "100");
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn transcript_ending_mid_tool_needs_input() {
        let ws = temp_ws("needs");
        write_session(
            &ws,
            "300",
            &[
                event(300, "task", json!({"text": "write b.txt :: x"})),
                event(301, "tool_call", json!({"step": 1, "tool": "fs.write", "args": {"path": "b.txt"}})),
            ],
        );
        let rows = scan_at(&ws, 400);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, State::NeedsInput);
        assert_eq!(rows[0].badges, vec![Badge::NeedsInput]);
        assert_eq!(rows[0].activity.as_deref(), Some("Writing b.txt"));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn filter_cycles_and_matches_states() {
        let row = |state: State| SessionRow {
            id: "x".into(),
            title: "t".into(),
            subtitle: None,
            state,
            activity: None,
            badges: Vec::new(),
            cwd: "~/x".into(),
            last_change: 0,
            steps: 0,
            tool_calls: 0,
            denied: 0,
        };
        assert_eq!(Filter::All.next(), Filter::Failed);
        assert_eq!(Filter::Failed.next(), Filter::Running);
        assert_eq!(Filter::Running.next(), Filter::All);

        assert!(Filter::All.matches(&row(State::Idle)));
        assert!(Filter::Failed.matches(&row(State::Failed)));
        assert!(!Filter::Failed.matches(&row(State::Done)));
        assert!(Filter::Running.matches(&row(State::Running)));
        assert!(Filter::Running.matches(&row(State::NeedsInput)));
        assert!(!Filter::Running.matches(&row(State::Failed)));
    }

    #[test]
    fn home_is_abbreviated_and_deep_paths_shortened() {
        assert_eq!(
            shorten_location("/home/karndt/projects/ArndtAi", Some("/home/karndt")),
            "~/projects/ArndtAi",
            "dwa ostatnie komponenty zostaja w calosci"
        );
        assert_eq!(
            shorten_location("/home/karndt/projects/ArndtAi/app", Some("/home/karndt")),
            "~/p/ArndtAi/app",
            "glebsza sciezka skraca pierwszy komponent"
        );
        assert_eq!(shorten_location("/home/karndt", Some("/home/karndt")), "~");
        assert_eq!(shorten_location("/opt/x", Some("/home/karndt")), "/opt/x");
    }

    #[test]
    fn age_uses_pager_buckets() {
        assert_eq!(format_age(0), "teraz");
        assert_eq!(format_age(59), "teraz");
        assert_eq!(format_age(60), "1m");
        assert_eq!(format_age(3_600), "1h");
        assert_eq!(format_age(86_400), "1d");
        assert_eq!(format_age(86_400 * 40), "1mo");
        assert_eq!(format_age(86_400 * 400), "1y");
    }

    #[test]
    fn linked_worktree_is_detected_from_dot_git_file() {
        let ws = temp_ws("worktree");
        std::fs::write(ws.join(".git"), "gitdir: /tmp/main/.git/worktrees/wt\n").unwrap();
        std::fs::create_dir_all("/tmp/arndtai-gitdir").ok();
        write_session(
            &ws,
            "400",
            &[
                event(400, "task", json!({"text": "cokolwiek"})),
                event(401, "summary", json!({"status": "DONE", "steps": 0, "tool_calls": 0, "denied": 0})),
            ],
        );
        let rows = scan_at(&ws, 500);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].badges, vec![Badge::Worktree]);
        assert!(is_linked_worktree(&ws));
        let _ = std::fs::remove_dir_all(&ws);
    }

    #[test]
    fn rows_sort_by_group_then_recency() {
        let plain = |id: &str, state: State, at: u64| SessionRow {
            id: id.into(),
            title: "t".into(),
            subtitle: None,
            state,
            activity: None,
            badges: Vec::new(),
            cwd: "~/x".into(),
            last_change: at,
            steps: 0,
            tool_calls: 0,
            denied: 0,
        };
        let mut rows = vec![
            plain("a", State::Failed, 900),
            plain("b", State::Done, 500),
            plain("c", State::NeedsInput, 100),
            plain("d", State::Running, 700),
            plain("e", State::Done, 800),
        ];
        sort_rows(&mut rows);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "d", "e", "b", "a"]);
    }
}
