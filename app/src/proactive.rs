//! Proaktywność na regułach lokalnych (`06-PROACTIVITY-AND-SELF-IMPROVEMENT.md`).
//!
//! Zasady ze specu, przeniesione na to, co da się zrobić bez nowych zależności:
//! - **default-deny**: brak pasującej reguły = wpis w logu i porzucenie,
//! - reguły w `policies/*.json` (spec mówi `*.yaml`; YAML wymagałby zależności
//!   — ten sam zestaw pól, format JSON),
//! - **shadow**: reguła z `shadow_days` najpierw tylko loguje, nie działa,
//! - brak decyzji modelu: wyzwalacz jest lokalny (status ostatniej sesji,
//!   odmowy bramek, cisza), a efekt to notatka i wpis w outboxie.
//!
//! `ardntai tick` jest idempotentny: ten sam wyzwalacz nie wywołuje drugi raz
//! tej samej reguły w oknie `cooldown_hours`.

use crate::context;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Policy {
    pub name: String,
    /// `session_failed`, `denied`, `idle_hours`.
    pub when: String,
    /// Dla `idle_hours` — próg w godzinach; dla `denied` — minimalna liczba.
    pub threshold: u64,
    pub cooldown_hours: u64,
    /// 0 = działa od razu; >0 = tylko log przez tyle dni od `created`.
    pub shadow_days: u64,
    /// Godziny ciszy (UTC): `[od, do]`, także przez północ (`[22, 6]`).
    pub quiet_hours: Option<(u64, u64)>,
    /// Górny limit notatek w oknie 24 h, licząc wszystkie reguły.
    pub max_per_day: u64,
    /// Zadanie do uruchomienia w sesji `worker-<name>-*` (tylko `tick --run`).
    pub action: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct Signal {
    pub last_status: String,
    pub denied: u64,
    pub idle_hours: u64,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub policy: String,
    pub shadow: bool,
    pub effect: String,
}

fn num(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|v| v.as_str())
}

/// Reguły z `policies/` (posortowane po nazwie pliku, żeby kolejność była stała).
pub fn load_policies(workspace: &Path) -> Vec<Policy> {
    let dir = workspace.join("policies");
    let mut files: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect(),
        Err(_) => return Vec::new(),
    };
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let name = text(&value, "name")
            .map(|s| s.to_string())
            .or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "bez-nazwy".to_string());
        let Some(when) = text(&value, "when") else {
            continue;
        };
        let quiet_hours = value
            .get("quiet_hours")
            .and_then(|v| v.as_array())
            .and_then(|a| {
                let from = a.first()?.as_u64()?;
                let to = a.get(1)?.as_u64()?;
                Some((from.min(23), to.min(24)))
            });
        out.push(Policy {
            name,
            when: when.to_string(),
            threshold: num(&value, "threshold"),
            cooldown_hours: num(&value, "cooldown_hours").max(1),
            shadow_days: num(&value, "shadow_days"),
            quiet_hours,
            max_per_day: num(&value, "max_per_day"),
            action: text(&value, "action")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            note: text(&value, "note").unwrap_or("").to_string(),
        });
    }
    out
}

/// Sygnały czytane z ostatniej sesji i z zegara — bez modelu i bez sieci.
pub fn signals(workspace: &Path, now: u64) -> Signal {
    let sessions = context::sessions(workspace);
    let mut last_status = String::from("brak");
    let mut denied = 0u64;
    let mut newest_mtime = 0u64;
    if let Some(path) = sessions.last() {
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                newest_mtime = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
            }
        }
        if let Ok(raw) = fs::read_to_string(path) {
            for line in raw.lines() {
                let Ok(ev) = serde_json::from_str::<Value>(line) else {
                    continue;
                };
                match ev.get("kind").and_then(|v| v.as_str()) {
                    Some("summary") => {
                        last_status = ev
                            .get("data")
                            .and_then(|d| d.get("status"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("brak")
                            .to_string();
                        denied += ev
                            .get("data")
                            .and_then(|d| d.get("denied"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);
                    }
                    Some("denied") => denied += 1,
                    _ => {}
                }
            }
        }
    }
    // Perimetr pisze surowe odmowy do `.agent/trigger_inbox.jsonl` (spec 06).
    // To drugie zrodlo sygnalu: odmowa bramki nie zawsze trafia do summary.
    if let Ok(raw) = fs::read_to_string(workspace.join(".agent").join("trigger_inbox.jsonl")) {
        for line in raw.lines() {
            if let Ok(ev) = serde_json::from_str::<Value>(line) {
                if ev.get("kind").and_then(|v| v.as_str()) == Some("denied") {
                    denied += 1;
                }
            }
        }
    }
    let idle_hours = if newest_mtime == 0 || newest_mtime > now {
        0
    } else {
        (now - newest_mtime) / 3600
    };
    Signal {
        last_status,
        denied,
        idle_hours,
    }
}

fn matches(policy: &Policy, signal: &Signal) -> bool {
    match policy.when.as_str() {
        "session_failed" => signal.last_status.starts_with("FAILED"),
        "denied" => signal.denied >= policy.threshold.max(1),
        "idle_hours" => signal.idle_hours >= policy.threshold.max(1),
        _ => false,
    }
}

fn read_log(workspace: &Path) -> Vec<Value> {
    let path = workspace.join(".agent").join("policies.log");
    fs::read_to_string(path)
        .map(|raw| {
            raw.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Czy `now` wpada w okno ciszy. Okno przechodzące przez północ (`22 → 6`)
/// jest sumą dwóch przedziałów, nie pustym zbiorem.
fn in_quiet_hours(now: u64, from: u64, to: u64) -> bool {
    let hour = (now % 86_400) / 3600;
    if from == to {
        return false;
    }
    if from < to {
        hour >= from && hour < to
    } else {
        hour >= from || hour < to
    }
}

fn append_log(workspace: &Path, entry: &Value) {
    let dir = workspace.join(".agent");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("policies.log");
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{entry}");
    }
}

fn append_note(workspace: &Path, policy: &str, text: &str) {
    let dir = workspace.join(".agent-memory");
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("notes.md");
    let fresh = !path.exists();
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        if fresh {
            let _ = f.write_all(b"# Notatki proaktywne\n\n");
        }
        let _ = writeln!(f, "- [{}] {}", policy, text);
    }
}

/// Jeden przebieg: dla każdej reguły sprawdź wyzwalacz, poszanuj cooldown
/// i shadow, zapisz efekt. Zwraca trafienia (także te w shadow) do wydruku.
pub fn tick(workspace: &Path, now: u64) -> Vec<Hit> {
    let policies = load_policies(workspace);
    let signal = signals(workspace, now);
    let log = read_log(workspace);
    let mut hits = Vec::new();

    // Budżet uwagi jest globalny: najniższy `max_per_day` wśród reguł wyznacza
    // limit notatek w oknie 24 h, licząc też to, co wyjdzie w tym przebiegu.
    let budget = policies
        .iter()
        .filter(|p| p.max_per_day > 0)
        .map(|p| p.max_per_day)
        .min()
        .unwrap_or(0);
    let recent_notes = log
        .iter()
        .filter(|entry| {
            let at = entry.get("at").and_then(|v| v.as_u64()).unwrap_or(0);
            entry.get("shadow").and_then(|v| v.as_bool()) != Some(true)
                && entry
                    .get("effect")
                    .and_then(|v| v.as_str())
                    .map(|e| !e.starts_with("brak") && !e.starts_with("cisza") && !e.starts_with("budzet"))
                    .unwrap_or(false)
                && now.saturating_sub(at) < 86_400
        })
        .count() as u64;
    let mut written = 0u64;

    if policies.is_empty() {
        append_log(
            workspace,
            &json!({"at": now, "policy": "-", "effect": "brak reguly — porzucone", "shadow": false}),
        );
        return hits;
    }

    for policy in policies {
        if !matches(&policy, &signal) {
            continue;
        }
        let cooldown = policy.cooldown_hours * 3600;
        let recent = log.iter().rev().any(|entry| {
            let same = entry.get("policy").and_then(|v| v.as_str()) == Some(&policy.name);
            let at = entry.get("at").and_then(|v| v.as_u64()).unwrap_or(0);
            let fired = entry.get("shadow").and_then(|v| v.as_bool()) != Some(true);
            same && fired && now.saturating_sub(at) < cooldown
        });
        if recent {
            continue;
        }

        // Godziny ciszy: reguła nie wychodzi, ale zostawia ślad, że wyzwalacz był.
        if let Some((from, to)) = policy.quiet_hours {
            if in_quiet_hours(now, from, to) {
                append_log(
                    workspace,
                    &json!({"at": now, "policy": policy.name, "effect": "cisza — wstrzymane", "shadow": false}),
                );
                continue;
            }
        }

        if budget > 0 && recent_notes + written >= budget {
            append_log(
                workspace,
                &json!({"at": now, "policy": policy.name, "effect": "budzet uwagi — wstrzymane", "shadow": false}),
            );
            continue;
        }

        let created = workspace.join("policies").join(format!("{}.json", policy.name));
        let created_at = fs::metadata(&created)
            .and_then(|m| m.created())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let shadow = policy.shadow_days > 0
            && created_at > 0
            && now.saturating_sub(created_at) < policy.shadow_days * 24 * 3600;

        let ripple = format!(
            "{} (status={}, odmowy={}, cisza={}h)",
            policy.note, signal.last_status, signal.denied, signal.idle_hours
        );
        if shadow {
            append_log(
                workspace,
                &json!({"at": now, "policy": policy.name, "effect": format!("shadow: {ripple}"), "shadow": true}),
            );
        } else {
            append_note(workspace, &policy.name, &ripple);
            written += 1;
            append_log(
                workspace,
                &json!({"at": now, "policy": policy.name, "effect": ripple, "shadow": false}),
            );
        }
        hits.push(Hit {
            policy: policy.name,
            shadow,
            effect: ripple,
        });
    }
    // Default-deny: gdy nic nie trafiło, zostaje ślad, że wyzwalacze sprawdzono.
    if hits.is_empty() {
        append_log(
            workspace,
            &json!({"at": now, "policy": "-", "effect": "brak trafien", "shadow": false}),
        );
    }
    hits
}

/// `tick --run`: po zapisaniu notatki uruchamia `action` w tle jako sesję
/// roboczą. Równoległość pilnuje `run.lock` rdzenia — jeśli ktoś już pracuje
/// w tym workspace, worker nie startuje (log „zajete"), a nie czeka w kolejce.
pub fn tick_run(workspace: &Path, now: u64, binary: &Path) -> Vec<Hit> {
    let hits = tick(workspace, now);
    if hits.is_empty() {
        return hits;
    }
    let policies = load_policies(workspace);
    for hit in &hits {
        if hit.shadow {
            continue;
        }
        let Some(policy) = policies.iter().find(|p| p.name == hit.policy) else {
            continue;
        };
        let Some(action) = policy.action.clone() else {
            continue;
        };
        let lock = workspace.join(".agent").join("run.lock");
        let busy = fs::read_to_string(&lock)
            .ok()
            .and_then(|raw| raw.trim().parse::<u32>().ok())
            .map(|pid| Path::new(&format!("/proc/{pid}")).exists())
            .unwrap_or(false);
        if busy {
            append_log(
                workspace,
                &json!({"at": now, "policy": hit.policy, "effect": "worker: zajete", "shadow": false}),
            );
            continue;
        }
        let spawned = std::process::Command::new(binary)
            .args(["run", "-w"])
            .arg(workspace)
            .arg(&action)
            .env("ARNDTAI_WORKER", &policy.name)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match spawned {
            Ok(child) => append_log(
                workspace,
                &json!({"at": now, "policy": hit.policy, "effect": format!("worker: start pid {}", child.id()), "shadow": false}),
            ),
            Err(error) => append_log(
                workspace,
                &json!({"at": now, "policy": hit.policy, "effect": format!("worker: blad {error}"), "shadow": false}),
            ),
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-proactive-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("policies")).unwrap();
        fs::create_dir_all(dir.join(".agent").join("sessions")).unwrap();
        dir
    }

    fn write_session(dir: &Path, status: &str, denied: u64) {
        let line = json!({"at": 100, "kind": "summary", "data": {"status": status, "denied": denied}});
        fs::write(
            dir.join(".agent").join("sessions").join("s1.jsonl"),
            format!("{line}\n"),
        )
        .unwrap();
    }

    #[test]
    fn without_a_policy_nothing_happens_but_the_log_says_so() {
        let dir = setup("brak");
        write_session(&dir, "FAILED_ERROR", 0);
        let hits = tick(&dir, 1000);
        assert!(hits.is_empty());
        assert!(!dir.join(".agent-memory").join("notes.md").exists());
        let log = fs::read_to_string(dir.join(".agent").join("policies.log")).unwrap();
        assert!(log.contains("brak reguly"), "log: {log}");
    }

    #[test]
    fn a_failed_session_fires_once_and_respects_the_cooldown() {
        let dir = setup("failed");
        write_session(&dir, "FAILED_ERROR", 0);
        fs::write(
            dir.join("policies").join("failed.json"),
            r#"{"name":"failed","when":"session_failed","cooldown_hours":6,"note":"zadanie padlo"}"#,
        )
        .unwrap();
        let hits = tick(&dir, 1000);
        assert_eq!(hits.len(), 1);
        let notes = fs::read_to_string(dir.join(".agent-memory").join("notes.md")).unwrap();
        assert!(notes.contains("zadanie padlo"), "notatka: {notes}");
        assert!(notes.contains("FAILED_ERROR"), "notatka bez statusu: {notes}");

        let again = tick(&dir, 1000 + 60);
        assert!(again.is_empty(), "cooldown ma trzymac: {again:?}");

        let later = tick(&dir, 1000 + 7 * 3600);
        assert_eq!(later.len(), 1, "po cooldownie regula dziala znow");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn quiet_hours_suppress_the_note_and_leave_a_log_entry() {
        let dir = setup("cisza");
        write_session(&dir, "FAILED_ERROR", 0);
        // 23:00 UTC wchodzi w okno 22-6.
        let now = 23 * 3600;
        fs::write(
            dir.join("policies").join("failed.json"),
            r#"{"name":"failed","when":"session_failed","cooldown_hours":1,"quiet_hours":[22,6],"note":"padlo"}"#,
        )
        .unwrap();
        // Sesja musi byc "swieza", inaczej idle liczy sie inaczej — tu nieistotne.
        let hits = tick(&dir, now);
        assert!(hits.is_empty(), "w godzinach ciszy nic nie wychodzi: {hits:?}");
        assert!(!dir.join(".agent-memory").join("notes.md").exists());
        let log = fs::read_to_string(dir.join(".agent").join("policies.log")).unwrap();
        assert!(log.contains("cisza"), "log ma mowic o ciszy: {log}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn attention_budget_caps_notes_per_day_across_policies() {
        let dir = setup("budzet");
        write_session(&dir, "FAILED_ERROR", 3);
        fs::write(
            dir.join("policies").join("a.json"),
            r#"{"name":"a","when":"session_failed","cooldown_hours":1,"max_per_day":1,"note":"pierwsza"}"#,
        )
        .unwrap();
        fs::write(
            dir.join("policies").join("b.json"),
            r#"{"name":"b","when":"denied","threshold":1,"cooldown_hours":1,"max_per_day":1,"note":"druga"}"#,
        )
        .unwrap();
        let hits = tick(&dir, 1000);
        assert_eq!(hits.len(), 1, "budzet uwagi: jedna notatka na okno: {hits:?}");
        let notes = fs::read_to_string(dir.join(".agent-memory").join("notes.md")).unwrap();
        assert_eq!(notes.matches("- [").count(), 1, "notatki: {notes}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn trigger_inbox_from_the_perimeter_feeds_the_denied_signal() {
        let dir = setup("inbox");
        // Transkrypt bez odmow, ale perimetr zapisal dwie surowe odmowy.
        let line = json!({"at": 100, "kind": "summary", "data": {"status": "DONE", "denied": 0}});
        fs::write(
            dir.join(".agent").join("sessions").join("s1.jsonl"),
            format!("{line}\n"),
        )
        .unwrap();
        fs::write(
            dir.join(".agent").join("trigger_inbox.jsonl"),
            "{\"at\":10,\"kind\":\"denied\",\"tool\":\"fs.write\",\"reason\":\"scope\"}\n\
             {\"at\":11,\"kind\":\"denied\",\"tool\":\"fs.read\",\"reason\":\"scope\"}\n",
        )
        .unwrap();
        fs::write(
            dir.join("policies").join("denied.json"),
            r#"{"name":"denied","when":"denied","threshold":2,"cooldown_hours":1,"note":"dwie odmowy"}"#,
        )
        .unwrap();
        let hits = tick(&dir, 1000);
        assert_eq!(hits.len(), 1, "inbox ma byc sygnalem: {hits:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tick_run_without_action_starts_nothing() {
        let dir = setup("run-brak");
        write_session(&dir, "FAILED_ERROR", 0);
        fs::write(
            dir.join("policies").join("failed.json"),
            r#"{"name":"failed","when":"session_failed","cooldown_hours":1,"note":"padlo"}"#,
        )
        .unwrap();
        let before = fs::read_dir(dir.join(".agent").join("sessions")).unwrap().count();
        let hits = tick_run(&dir, 1000, Path::new("/bin/true"));
        assert_eq!(hits.len(), 1);
        let after = fs::read_dir(dir.join(".agent").join("sessions")).unwrap().count();
        assert_eq!(before, after, "bez action nie ma nowej sesji");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tick_run_respects_a_live_lock() {
        let dir = setup("run-lock");
        write_session(&dir, "FAILED_ERROR", 0);
        fs::write(
            dir.join("policies").join("failed.json"),
            r#"{"name":"failed","when":"session_failed","cooldown_hours":1,"action":"write a.txt :: worker","note":"padlo"}"#,
        )
        .unwrap();
        fs::write(
            dir.join(".agent").join("run.lock"),
            format!("{}\n", std::process::id()),
        )
        .unwrap();
        tick_run(&dir, 1000, Path::new("/bin/true"));
        let log = fs::read_to_string(dir.join(".agent").join("policies.log")).unwrap();
        assert!(log.contains("worker: zajete"), "log: {log}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shadow_policy_only_logs() {
        let dir = setup("shadow");
        write_session(&dir, "FAILED_STALLED", 2);
        fs::write(
            dir.join("policies").join("denied.json"),
            r#"{"name":"denied","when":"denied","threshold":1,"cooldown_hours":1,"shadow_days":14,"note":"sa odmowy"}"#,
        )
        .unwrap();
        let hits = tick(&dir, 1000);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].shadow, "swieza regula ma byc w shadow");
        assert!(!dir.join(".agent-memory").join("notes.md").exists());
        let log = fs::read_to_string(dir.join(".agent").join("policies.log")).unwrap();
        assert!(log.contains("shadow"), "log: {log}");
        let _ = fs::remove_dir_all(&dir);
    }
}
