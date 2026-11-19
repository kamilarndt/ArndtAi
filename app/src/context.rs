//! Kontekst agenta: osobowość, skille i pamięć projektu plus transkrypty sesji.
//!
//! Poziomy pamięci wg docs/02-memory.md:
//! - Lite: surowy log sesji + wyszukiwanie (project.md).
//! - Pro:  + fakty i epizody (facts.md, episodes.md).
//! - Ultra: + konsolidacja (graph.md).

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_PERSONA: &str = "\
Jestes ArndtAi — dzienny doradca Kamila. Mow po polsku, wprost, po kumplowemu.\
 Pierwsze zdanie to wynik albo decyzja, nie temat. Domyslnie 3-5 linii.\
 Zero emoji. Kazda liczba musi pochodzic z narzedzia, nie z pamieci.\
 Brak danych: mowisz czego brakuje i skad to wezmiesz. Nie wymyslaj wynikow.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemoryLevel {
    Lite,
    Pro,
    Ultra,
}

impl MemoryLevel {
    pub fn parse(text: &str) -> Option<MemoryLevel> {
        match text.trim().to_ascii_lowercase().as_str() {
            "lite" => Some(MemoryLevel::Lite),
            "pro" => Some(MemoryLevel::Pro),
            "ultra" => Some(MemoryLevel::Ultra),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryLevel::Lite => "lite",
            MemoryLevel::Pro => "pro",
            MemoryLevel::Ultra => "ultra",
        }
    }
}

/// Skill widziany przez CLI/status — typ i odkrywanie w [`crate::skills`].
pub use crate::skills::Skill;

pub struct Prefix {
    pub persona: String,
    pub persona_source: String,
    pub skills: Vec<Skill>,
    pub memory: Option<String>,
    pub memory_source: Option<String>,
    pub memory_level: MemoryLevel,
}

impl Prefix {
    pub fn load(workspace: &Path, level: MemoryLevel) -> Prefix {
        let persona_candidates = [
            "personality/base.md",
            ".agent/personality/base.md",
            "personality.md",
            "PERSONALITY.md",
            "PERSONA.md",
            "persona.md",
            "arndtai/personality.md",
        ];
        let mut persona = DEFAULT_PERSONA.to_string();
        let mut persona_source = "wbudowana".to_string();
        for cand in persona_candidates.iter() {
            if let Some(text) = read_nonempty(&workspace.join(cand)) {
                persona = text;
                persona_source = cand.to_string();
                break;
            }
        }

        let mut parts: Vec<String> = Vec::new();
        let mut sources: Vec<String> = Vec::new();
        for cand in memory_candidates(level) {
            if let Some(text) = read_nonempty(&workspace.join(cand)) {
                parts.push(format!("--- {} ---\n{}", cand, text.trim()));
                sources.push(cand.to_string());
            }
        }
        let memory = if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n\n"))
        };
        let memory_source = if sources.is_empty() {
            None
        } else {
            Some(sources.join(", "))
        };

        Prefix {
            persona,
            persona_source,
            // Runda 16: kolejność katalogów i nagłówki YAML w `skills.rs`.
            skills: crate::skills::discover(workspace),
            memory,
            memory_source,
            memory_level: level,
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("=== OSOBOWOSC ===\n");
        out.push_str(self.persona.trim());
        out.push_str(&format!("\n(zrodlo: {})\n", self.persona_source));

        // Runda 16: lista `- nazwa: opis` idzie do promptu **systemowego**
        // (provider::system_prompt), więc tutaj jest tylko licznik — kontekst
        // nie płaci za nią dwa razy.
        out.push_str("\n=== SKILLE ===\n");
        if self.skills.is_empty() {
            out.push_str("brak skilli (skills/<nazwa>/SKILL.md)\n");
        } else {
            out.push_str(&format!(
                "{} skilli — lista w promptcie systemowym, tresc przez /skill <nazwa> lub skill.load\n",
                self.skills.len()
            ));
        }

        out.push_str(&format!(
            "\n=== PAMIEC PROJEKTU (poziom {}) ===\n",
            self.memory_level.as_str()
        ));
        match (&self.memory, &self.memory_source) {
            (Some(text), Some(src)) => {
                out.push_str(text);
                out.push_str(&format!("\n(zrodlo: {})\n", src));
            }
            _ => out.push_str("brak pliku pamieci\n"),
        }
        out
    }
}

/// Pliki pamięci czytane na danym poziomie, w kolejności.
fn memory_candidates(level: MemoryLevel) -> Vec<&'static str> {
    let mut out = vec![
        ".agent-memory/project.md",
        "memory/project.md",
        ".agent/memory/project.md",
    ];
    if matches!(level, MemoryLevel::Pro | MemoryLevel::Ultra) {
        out.push("memory/facts.md");
        out.push(".agent-memory/facts.md");
        out.push("memory/episodes.md");
        out.push(".agent-memory/episodes.md");
    }
    if level == MemoryLevel::Ultra {
        out.push("memory/graph.md");
        out.push(".agent-memory/graph.md");
    }
    out
}

fn read_nonempty(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub struct Session {
    pub id: String,
    pub file: PathBuf,
    /// Runda 11 (M1): ta sama linia, ktora idzie do JSONL, trafia tez do
    /// `transcripts` w SQLite (JSONL zostaje eksportem i kontraktem).
    store: Option<crate::store::Store>,
}

/// Strumien eventow dla konsumenta w tym samym procesie (TUI). Kontrakt i plik
/// JSONL zostaja bez zmian: sink dostaje dokladnie te sama linie, ktora wlasnie
/// poszla do transkryptu, tylko sparsowana.
pub type EventSink = std::sync::mpsc::Sender<Value>;

static EVENT_SINK: OnceLock<Mutex<Option<EventSink>>> = OnceLock::new();

fn sink_slot() -> &'static Mutex<Option<EventSink>> {
    EVENT_SINK.get_or_init(|| Mutex::new(None))
}

/// Podpiecie (albo odpiecie, `None`) strumienia eventow. Jeden konsument;
/// zamkniecie kanalu automatycznie odpina sink, zeby petla nie pisala w prozne.
pub fn set_event_sink(tx: Option<EventSink>) {
    if let Ok(mut slot) = sink_slot().lock() {
        *slot = tx;
    }
}

fn publish(line: &str) {
    let Ok(mut slot) = sink_slot().lock() else {
        return;
    };
    let Some(tx) = slot.as_ref() else {
        return;
    };
    match serde_json::from_str::<Value>(line) {
        Ok(value) => {
            if tx.send(value).is_err() {
                *slot = None;
            }
        }
        Err(_) => {}
    }
}

impl Session {
    pub fn start(workspace: &Path, task: &str) -> Session {
        let dir = workspace.join(".agent").join("sessions");
        let _ = fs::create_dir_all(&dir);
        let ts = now_ts();
        let mut id = ts.to_string();
        let mut file = dir.join(format!("{}.jsonl", id));
        let mut n = 1;
        while file.exists() {
            id = format!("{}-{}", ts, n);
            file = dir.join(format!("{}.jsonl", id));
            n += 1;
        }
        let session = Session {
            id,
            file,
            store: Some(crate::store::Store::for_workspace(workspace)),
        };
        session.event("task", json!({ "text": task }));
        session
    }

    /// Wznowienie istniejacej sesji: ten sam `id`, ten sam plik transkryptu.
    /// Bez tego po restarcie powstaje nowy `session_id` i kroki `done` nigdy
    /// sie nie odnajda.
    pub fn resume(workspace: &Path, id: &str) -> Session {
        let dir = workspace.join(".agent").join("sessions");
        let _ = fs::create_dir_all(&dir);
        Session {
            id: id.to_string(),
            file: dir.join(format!("{}.jsonl", id)),
            store: Some(crate::store::Store::for_workspace(workspace)),
        }
    }

    pub fn event(&self, kind: &str, data: Value) {
        // Krok jest czescia `data` eventu; kolumna `step` tylko go wynosi.
        let step = data
            .get("step")
            .and_then(|v| v.as_u64())
            .map(|s| s.min(u32::MAX as u64) as u32);
        let line = json!({ "at": now_ts(), "kind": kind, "data": data }).to_string();
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.file)
        {
            use std::io::Write;
            let _ = f.write_all(format!("{}\n", line).as_bytes());
        }
        // Ta sama tresc w SQLite; blad zapisu nie zatrzymuje petli (JSONL jest
        // zapisem pierwszym, baza jest indeksem).
        if let Some(store) = &self.store {
            let _ = store.append_transcript(&self.id, step, kind, &line);
        }
        // Ta sama linia, bez zmiany formatu, trafia do TUI (jesli podpiety).
        publish(&line);
    }
}

/// Lista transkryptów w workspace, od najstarszego.
pub fn sessions(workspace: &Path) -> Vec<PathBuf> {
    let dir = workspace.join(".agent").join("sessions");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|x| x.to_str()) == Some("jsonl") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}
