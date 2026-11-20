//! Self-improvement ograniczony do artefaktów deklaratywnych (`06`, sekcja
//! "Self-improvement"): analiza transkryptów → propozycja pliku → **nigdy**
//! automatyczne zastosowanie. Dziś jedyny artefakt, który umiemy zaproponować
//! bezpiecznie, to reguła proaktywna (`policies/*.json`) wyprowadzona
//! z powtarzających się odmów bramek.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Proposal {
    pub kind: String,
    pub title: String,
    pub body: String,
}

/// Reguła proponowana, gdy ta sama odmowa powtarza się co najmniej 3 razy.
const MIN_REPEATS: u64 = 3;

/// Czytaj odmowy z transkryptów i z `trigger_inbox` perimetru.
fn denial_counts(workspace: &Path) -> BTreeMap<String, u64> {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    let sessions = crate::context::sessions(workspace);
    for path in sessions.iter().rev().take(20) {
        let Ok(raw) = fs::read_to_string(path) else {
            continue;
        };
        for line in raw.lines() {
            let Ok(ev) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if ev.get("kind").and_then(|v| v.as_str()) != Some("denied") {
                continue;
            }
            let reason = ev
                .get("data")
                .and_then(|d| d.get("reason"))
                .and_then(|v| v.as_str())
                .unwrap_or("brak powodu")
                .to_string();
            *counts.entry(reason).or_insert(0) += 1;
        }
    }
    if let Ok(raw) = fs::read_to_string(workspace.join(".agent").join("trigger_inbox.jsonl")) {
        for line in raw.lines() {
            let Ok(ev) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if ev.get("kind").and_then(|v| v.as_str()) != Some("denied") {
                continue;
            }
            let reason = ev
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("brak powodu")
                .to_string();
            *counts.entry(format!("inbox: {reason}")).or_insert(0) += 1;
        }
    }
    counts
}

pub fn propose(workspace: &Path) -> Vec<Proposal> {
    let mut out = Vec::new();
    for (reason, count) in denial_counts(workspace) {
        if count < MIN_REPEATS {
            continue;
        }
        let name = format!("powtarzajace-sie-odmowy-{}", slug(&reason));
        let body = json!({
            "name": name,
            "when": "denied",
            "threshold": count.min(10),
            "cooldown_hours": 12,
            "shadow_days": 14,
            "note": format!("Powtarzajaca sie odmowa ({count}x): {reason}")
        });
        out.push(Proposal {
            kind: "policy".to_string(),
            title: format!("Reguła dla odmowy: {reason}"),
            body: serde_json::to_string_pretty(&body).unwrap_or_else(|_| "{}".to_string()),
        });
    }
    out
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars().take(40) {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Zapisuje propozycje do `docs/proposals/`. Świadomie **nie** dotyka
/// `policies/` ani `personality/`: zastosowanie zostaje decyzją człowieka.
pub fn apply(workspace: &Path, proposals: &[Proposal], now: u64) -> Result<Vec<PathBuf>, String> {
    if proposals.is_empty() {
        return Ok(Vec::new());
    }
    let dir = workspace.join("docs").join("proposals");
    fs::create_dir_all(&dir).map_err(|e| format!("brak docs/proposals: {e}"))?;
    let mut saved = Vec::new();
    for (i, proposal) in proposals.iter().enumerate() {
        let path = dir.join(format!("{now}-{}-{i}.md", slug(&proposal.kind)));
        let text = format!(
            "# Propozycja: {}\n\n- rodzaj: `{}`\n- wygenerowana: {}\n\n```json\n{}\n```\n\n\
             Zastosowanie: przenieś plik do `policies/` po przejrzeniu (shadow 14 dni).\n",
            proposal.title, proposal.kind, now, proposal.body
        );
        fs::write(&path, text).map_err(|e| format!("nie moge zapisac propozycji: {e}"))?;
        saved.push(path);
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-improve-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".agent").join("sessions")).unwrap();
        dir
    }

    fn denied_session(dir: &Path, id: &str, reason: &str) {
        let line = json!({
            "at": 100,
            "kind": "denied",
            "data": {"step": 1, "tool": "fs.write", "reason": reason, "drift": false}
        });
        fs::write(
            dir.join(".agent").join("sessions").join(format!("{id}.jsonl")),
            format!("{line}\n"),
        )
        .unwrap();
    }

    #[test]
    fn no_transcripts_means_no_proposals() {
        let dir = setup("pusto");
        assert!(propose(&dir).is_empty());
        assert!(apply(&dir, &[], 1000).unwrap().is_empty());
        assert!(!dir.join("docs").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn repeated_denials_produce_a_policy_proposal() {
        let dir = setup("odmowy");
        denied_session(&dir, "1", "scope");
        denied_session(&dir, "2", "scope");
        denied_session(&dir, "3", "scope");
        let proposals = propose(&dir);
        assert_eq!(proposals.len(), 1, "{proposals:?}");
        assert_eq!(proposals[0].kind, "policy");
        assert!(proposals[0].body.contains("\"when\": \"denied\""), "{}", proposals[0].body);
        assert!(proposals[0].body.contains("\"threshold\": 3"), "{}", proposals[0].body);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_writes_proposals_and_never_touches_policies() {
        let dir = setup("apply");
        denied_session(&dir, "1", "scope");
        denied_session(&dir, "2", "scope");
        denied_session(&dir, "3", "scope");
        let proposals = propose(&dir);
        let saved = apply(&dir, &proposals, 1000).unwrap();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].starts_with(dir.join("docs").join("proposals")));
        assert!(!dir.join("policies").exists(), "apply nie tworzy regul");
        let _ = fs::remove_dir_all(&dir);
    }
}
