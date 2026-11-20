//! Konsolidacja pamięci Ultra (`02-MEMORY.md`: graf, schematy, intencje).
//!
//! Deterministyczna, bez modelu i bez sieci: bierze surowe fakty i epizody
//! z `.agent-memory/` i grupuje je w `memory/graph.md` po temacie. To nie jest
//! "myślenie" — to porządek, po którym `ultra` widzi, czego jest dużo,
//! a czego brakuje. Pusty magazyn nie tworzy pliku.

use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Report {
    pub facts: usize,
    pub episodes: usize,
    pub clusters: usize,
    pub graph: Option<PathBuf>,
}

/// Tematy rozpoznawane po słowach w treści; reszta trafia do `inne`.
const TOPICS: [&str; 5] = ["pamiec", "web", "narzedzia", "bezpieczenstwo", "inne"];

fn read_bullets(path: &Path) -> Vec<String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| l.starts_with("- "))
        .map(|l| l.trim_start_matches("- ").trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn topic_of(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    for topic in TOPICS.iter().take(TOPICS.len() - 1) {
        if lower.contains(topic) {
            return topic;
        }
    }
    // Bliskie warianty polskich słów bez znaków diakrytycznych.
    if lower.contains("pami") {
        return "pamiec";
    }
    if lower.contains("siec") || lower.contains("http") {
        return "web";
    }
    "inne"
}

pub fn consolidate(workspace: &Path, _now: u64) -> Result<Report, String> {
    let memory = workspace.join(".agent-memory");
    let facts = read_bullets(&memory.join("facts.md"));
    let episodes = read_bullets(&memory.join("episodes.md"));
    let mut clusters: BTreeMap<&'static str, Vec<String>> = BTreeMap::new();
    for line in facts.iter().chain(episodes.iter()) {
        clusters.entry(topic_of(line)).or_default().push(line.clone());
    }

    if facts.is_empty() && episodes.is_empty() {
        return Ok(Report {
            facts: 0,
            episodes: 0,
            clusters: 0,
            graph: None,
        });
    }

    let dir = workspace.join("memory");
    fs::create_dir_all(&dir).map_err(|e| format!("brak katalogu memory: {e}"))?;
    let path = dir.join("graph.md");
    let mut out = String::from("# Graf pamięci (ultra)\n\n");
    out.push_str("Klastry tematyczne z `.agent-memory/facts.md` i `episodes.md`.\n\n");

    let mut ordered: Vec<(&'static str, Vec<String>)> = clusters.into_iter().collect();
    ordered.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    for (topic, items) in &ordered {
        out.push_str(&format!("## {} ({})\n\n", topic, items.len()));
        for item in items.iter().take(5) {
            out.push_str(&format!("- {}\n", item));
        }
        if items.len() > 5 {
            out.push_str(&format!("- … i {} więcej\n", items.len() - 5));
        }
        out.push('\n');
    }
    fs::write(&path, out).map_err(|e| format!("nie moge zapisac graph.md: {e}"))?;
    Ok(Report {
        facts: facts.len(),
        episodes: episodes.len(),
        clusters: ordered.len(),
        graph: Some(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-consolidate-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".agent-memory")).unwrap();
        dir
    }

    #[test]
    fn empty_workspace_writes_nothing() {
        let dir = setup("pusty");
        let report = consolidate(&dir, 1000).unwrap();
        assert_eq!(report.facts, 0);
        assert_eq!(report.clusters, 0);
        assert!(!dir.join("memory").join("graph.md").exists());
    }

    #[test]
    fn facts_and_episodes_become_topic_clusters() {
        let dir = setup("klastry");
        fs::write(
            dir.join(".agent-memory").join("facts.md"),
            "# Fakty\n\n- pamiec pro czyta facts.md\n- pamiec ultra czyta graph.md\n- web UI czyta jsonl\n",
        )
        .unwrap();
        fs::write(
            dir.join(".agent-memory").join("episodes.md"),
            "# Epizody\n\n- 1 · DONE · pamiec zapisana\n- 2 · DONE · web uruchomiony\n",
        )
        .unwrap();
        let report = consolidate(&dir, 1000).unwrap();
        assert_eq!(report.facts, 3);
        assert_eq!(report.episodes, 2);
        assert_eq!(report.clusters, 2, "pamiec + web");
        let graph = fs::read_to_string(dir.join("memory").join("graph.md")).unwrap();
        assert!(graph.contains("## pamiec (3)"), "graf:\n{graph}");
        assert!(graph.contains("## web (2)"), "graf:\n{graph}");
        let _ = fs::remove_dir_all(&dir);
    }
}
