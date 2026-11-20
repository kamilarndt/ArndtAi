//! Second brain: lokalny vault w `.agent-memory/vault/`.
//!
//! Spec `02-MEMORY.md`: pusty na start, ingest tylko na jawne polecenie
//! (`ardntai ingest` albo narzędzie `memory.ingest`). Bez modelu i bez sieci:
//! kopiujemy tekst, nadajemy slug i dopisujemy wpis do `index.md`.

use std::fs;
use std::path::Path;

fn slugify(title: &str) -> String {
    let mut out = String::new();
    for ch in title.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
        } else if lower == ' ' || lower == '-' || lower == '_' {
            if !out.ends_with('-') && !out.is_empty() {
                out.push('-');
            }
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "notatka".to_string()
    } else {
        trimmed
    }
}

/// Kopiuje plik z workspace do vaulta i dopisuje go do `index.md`.
/// Zwraca ścieżkę względną zapisanego pliku.
pub fn ingest(workspace: &Path, rel_source: &str, title: &str) -> Result<String, String> {
    let source = crate::tools::resolve(workspace, rel_source).map_err(|e| {
        let detail = e
            .output
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("odrzucone przez bramke zakresu");
        format!("zrodlo poza workspace albo nieczytelne: {detail}")
    })?;
    let text = fs::read_to_string(&source)
        .map_err(|e| format!("nie moge przeczytac zrodla: {e}"))?;

    let dir = workspace.join(".agent-memory").join("vault");
    fs::create_dir_all(&dir).map_err(|e| format!("brak katalogu vaulta: {e}"))?;
    let slug = slugify(title);
    let name = format!("{slug}.md");
    let target = dir.join(&name);
    fs::write(&target, text).map_err(|e| format!("nie moge zapisac notatki: {e}"))?;

    let index = dir.join("index.md");
    let mut body = fs::read_to_string(&index).unwrap_or_default();
    if body.trim().is_empty() {
        body.push_str("# Second brain — index\n\n");
    }
    let entry = format!("- [{}]({})\n", title.trim(), name);
    if !body.contains(&entry) {
        body.push_str(&entry);
        fs::write(&index, body).map_err(|e| format!("nie moge zapisac index.md: {e}"))?;
    }
    Ok(format!(".agent-memory/vault/{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-vault-{}-{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ingest_copies_a_workspace_file_and_indexes_it() {
        let dir = setup("ingest");
        fs::write(dir.join("nota.md"), "# Nota\n\ntresc").unwrap();
        let saved = ingest(&dir, "nota.md", "Nota").unwrap();
        assert_eq!(saved, ".agent-memory/vault/nota.md");
        let copy = fs::read_to_string(dir.join(".agent-memory").join("vault").join("nota.md"));
        assert_eq!(copy.unwrap(), "# Nota\n\ntresc");
        let index = fs::read_to_string(dir.join(".agent-memory").join("vault").join("index.md")).unwrap();
        assert!(index.contains("[Nota](nota.md)"), "index:\n{index}");
        // Drugi ingest tego samego tytulu nie duplikuje wpisu.
        ingest(&dir, "nota.md", "Nota").unwrap();
        let index = fs::read_to_string(dir.join(".agent-memory").join("vault").join("index.md")).unwrap();
        assert_eq!(index.matches("[Nota](nota.md)").count(), 1, "index:\n{index}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn source_outside_the_workspace_is_refused() {
        let dir = setup("poza");
        let err = ingest(&dir, "../etc/passwd", "x").unwrap_err();
        assert!(err.contains("poza workspace"), "blad: {err}");
        assert!(!dir.join(".agent-memory").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_title_without_letters_still_gets_a_slug() {
        assert_eq!(slugify("!!! ---"), "notatka");
        assert_eq!(slugify("Moja Nota 2"), "moja-nota-2");
    }
}
