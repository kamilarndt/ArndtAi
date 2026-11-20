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
        out.push(Policy {
            name,
            when: when.to_string(),
            threshold: num(&value, "threshold"),
            cooldown_hours: num(&value, "cooldown_hours").max(1),
            shadow_days: num(&value, "shadow_days"),
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
