//! Eval gate (`06`, sekcja "Self-improvement"): Pass^k na zestawie przypadków
//! i auto-rollback dla zmian w `policies/`.
//!
//! Przypadki są plikami JSON w `evals/cases/`, uruchamianymi w osobnym
//! workspace dla każdego powtórzenia — kazdy przypadek to zadanie, pliki
//! wejściowe i oczekiwania wobec artefaktów. Provider jest offline (mock),
//! więc bramka jest deterministyczna i nie kosztuje tokenów.

use crate::agent::{self, Config};
use crate::context::MemoryLevel;
use crate::tools;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Case {
    pub name: String,
    pub task: String,
    /// Pliki wejściowe: nazwa → treść.
    pub files: Vec<(String, String)>,
    /// Oczekiwania: ścieżka i fragment treści, która musi wystąpić.
    pub expect: Vec<(String, String)>,
    /// Oczekiwany status przebiegu (domyślnie `DONE`).
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub case: String,
    pub runs: usize,
    pub passes: usize,
}

impl Outcome {
    pub fn pass_k(&self) -> bool {
        self.runs > 0 && self.passes == self.runs
    }
}

pub fn load_cases(workspace: &Path) -> Result<Vec<Case>, String> {
    let dir = workspace.join("evals").join("cases");
    let entries = fs::read_dir(&dir).map_err(|e| format!("brak evals/cases: {e}"))?;
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut cases = Vec::new();
    for path in paths {
        let raw = fs::read_to_string(&path).map_err(|e| format!("{path:?}: {e}"))?;
        let value: Value = serde_json::from_str(&raw).map_err(|e| format!("{path:?}: {e}"))?;
        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("{path:?}: brak name"))?;
        let task = value
            .get("task")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("{path:?}: brak task"))?;
        let files = value
            .get("files")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let expect = value
            .get("expect")
            .and_then(|v| v.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let status = value
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("DONE")
            .to_string();
        cases.push(Case {
            name,
            task,
            files,
            expect,
            status,
        });
    }
    Ok(cases)
}

/// Uruchamia przypadek `k` razy w czystym workspace. Każde powtórzenie ma
/// własny katalog, więc jedno nie widzi artefaktów drugiego.
pub fn run_case(base: &Path, case: &Case, k: usize) -> Outcome {
    let mut passes = 0;
    for i in 0..k {
        let dir = base
            .join(".agent")
            .join("eval")
            .join(format!("{}-{}", case.name, i));
        let _ = fs::remove_dir_all(&dir);
        if fs::create_dir_all(&dir).is_err() {
            continue;
        }
        for (name, content) in &case.files {
            let path = dir.join(name);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(path, content);
        }
        let cfg = Config {
            workspace: dir.clone(),
            max_steps: 8,
            allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
            provider: Some("mock".to_string()),
            quiet: true,
            mcp: false,
            memory: MemoryLevel::Lite,
            resume: false,
            model: None,
            max_tokens: None,
            approver: None,
            skill_context: Vec::new(),
        };
        let report = agent::run(&case.task, &cfg);
        let mut ok = report.status == case.status;
        if ok {
            for (rel, needle) in &case.expect {
                match fs::read_to_string(dir.join(rel)) {
                    Ok(text) if text.contains(needle.as_str()) => {}
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
        }
        if ok {
            passes += 1;
        }
        let _ = fs::remove_dir_all(&dir);
    }
    Outcome {
        case: case.name.clone(),
        runs: k,
        passes,
    }
}

/// Snapshot katalogu `policies/` — potrzebny do auto-rollbacku, gdy kandydująca
/// reguła obleje eval gate.
pub fn snapshot_policies(workspace: &Path) -> Result<Vec<(String, String)>, String> {
    let dir = workspace.join("policies");
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let body = fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))?;
        out.push((name, body));
    }
    Ok(out)
}

pub fn restore_policies(workspace: &Path, snapshot: &[(String, String)]) -> Result<(), String> {
    let dir = workspace.join("policies");
    fs::create_dir_all(&dir).map_err(|e| format!("brak policies: {e}"))?;
    // Usuń pliki, których nie było w snapshocie.
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if !snapshot.iter().any(|(kept, _)| kept == &name) {
                let _ = fs::remove_file(path);
            }
        }
    }
    for (name, body) in snapshot {
        fs::write(dir.join(name), body).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
}

/// Zapisuje kandydującą regułę, uruchamia eval gate i wycofuje zmianę, gdy
/// cokolwiek nie przejdzie. Zwraca `Ok(true)`, gdy reguła została.
pub fn gate_policy(workspace: &Path, name: &str, body: &str, k: usize) -> Result<bool, String> {
    let snapshot = snapshot_policies(workspace)?;
    let dir = workspace.join("policies");
    fs::create_dir_all(&dir).map_err(|e| format!("brak policies: {e}"))?;
    fs::write(dir.join(format!("{name}.json")), body).map_err(|e| format!("zapis: {e}"))?;

    let cases = load_cases(workspace).unwrap_or_default();
    let base = workspace.join(".agent").join("eval");
    let failed: Vec<String> = cases
        .iter()
        .map(|case| run_case(&base, case, k))
        .filter(|outcome| !outcome.pass_k())
        .map(|outcome| format!("{}({}/{})", outcome.case, outcome.passes, outcome.runs))
        .collect();
    // Brak przypadków to nie dowód: bez zestawu bramka nie przepuszcza zmiany.
    if cases.is_empty() || !failed.is_empty() {
        restore_policies(workspace, &snapshot)?;
        return Ok(false);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-eval-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("evals").join("cases")).unwrap();
        dir
    }

    fn write_case(dir: &Path, name: &str, task: &str, expect: Value) {
        let body = json!({"name": name, "task": task, "expect": expect});
        fs::write(
            dir.join("evals").join("cases").join(format!("{name}.json")),
            body.to_string(),
        )
        .unwrap();
    }

    #[test]
    fn a_matching_case_passes_k_times() {
        let dir = setup("pass");
        write_case(&dir, "zapis", "write a.txt :: alfa", json!({"a.txt": "alfa"}));
        let cases = load_cases(&dir).unwrap();
        let outcome = run_case(&dir.join(".agent").join("eval"), &cases[0], 2);
        assert_eq!(outcome.passes, 2, "{outcome:?}");
        assert!(outcome.pass_k());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_expectation_fails_and_is_visible() {
        let dir = setup("fail");
        write_case(&dir, "zapis", "write a.txt :: alfa", json!({"a.txt": "beta"}));
        let cases = load_cases(&dir).unwrap();
        let outcome = run_case(&dir.join(".agent").join("eval"), &cases[0], 1);
        assert_eq!(outcome.passes, 0);
        assert!(!outcome.pass_k());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_candidate_policy_is_rolled_back_when_the_gate_fails() {
        let dir = setup("rollback");
        fs::create_dir_all(dir.join("policies")).unwrap();
        fs::write(dir.join("policies").join("stara.json"), "{\"name\":\"stara\"}").unwrap();
        write_case(&dir, "zapis", "write a.txt :: alfa", json!({"a.txt": "beta"}));
        let kept = gate_policy(&dir, "kandydat", "{\"name\":\"kandydat\"}", 1).unwrap();
        assert!(!kept, "oblany gate musi wycofac kandydata");
        assert!(!dir.join("policies").join("kandydat.json").exists());
        assert!(dir.join("policies").join("stara.json").exists(), "stara regula wraca");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_cases_the_gate_refuses_the_change() {
        let dir = setup("brak-cases");
        fs::create_dir_all(dir.join("policies")).unwrap();
        let kept = gate_policy(&dir, "kandydat", "{\"name\":\"kandydat\"}", 1).unwrap();
        assert!(!kept, "brak przypadkow to brak dowodu");
        assert!(!dir.join("policies").join("kandydat.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
