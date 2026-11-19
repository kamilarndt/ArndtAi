//! Skille (runda 16): odkrywanie `skills/<nazwa>/SKILL.md`, nagłówek YAML
//! i treść wstrzykiwana do kontekstu.
//!
//! Zasady (NEXT runda 16):
//! - Katalogi w kolejności: `<workspace>/skills/`, `<katalog aplikacji>/skills/`,
//!   `~/.config/arndtai/skills/`. Ten sam `name` w późniejszym katalogu **nie**
//!   nadpisuje wcześniejszego — pierwszeństwo ma workspace.
//! - Nagłówek YAML w `---` z polami `name`, `description` (wymagane) i `when`
//!   (opcjonalne). Brak nagłówka → `name` = nazwa katalogu, `description` =
//!   pierwsza linia tekstu. Zepsuty nagłówek nie wywala startu: skill zostaje
//!   na liście z oznaczeniem, ale nie jest wstrzykiwany.
//! - Do promptu systemowego idzie lista `- <name>: <description>`, maks. 20
//!   pozycji, posortowana po nazwie. Pełna treść wchodzi do kontekstu tylko
//!   po `/skill <nazwa>` albo przez narzędzie `skill.load` — z limitem 8 KB.

use std::fs;
use std::path::{Path, PathBuf};

/// Ile pozycji listy wchodzi do promptu systemowego.
pub const MAX_LIST: usize = 20;
/// Limit treści jednego skilla w kontekście (bajty, jak w kontrakcie MCP).
pub const MAX_CONTENT_BYTES: usize = 8192;

/// Skąd skill przyszedł — widoczne w `/skills` i w `ardntai skills`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Workspace,
    App,
    Config,
}

impl Source {
    pub fn label(&self) -> &'static str {
        match self {
            Source::Workspace => "workspace",
            Source::App => "aplikacja",
            Source::Config => "config",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// `when` z nagłówka — kiedy używać; puste, gdy nie podano.
    pub when: Option<String>,
    /// Plik `SKILL.md` (katalog to `path.parent()`).
    pub path: PathBuf,
    /// Źródło odkrycia (kolejność katalogów).
    pub source: Source,
    /// Treść bez nagłówka — dokładnie to, co wchodzi do kontekstu.
    pub body: String,
    /// `Some(powód)`, gdy nagłówek jest zepsuty: skill jest na liście, ale
    /// nie jest wstrzykiwany.
    pub header_error: Option<String>,
}

impl Skill {
    pub fn broken(&self) -> bool {
        self.header_error.is_some()
    }

    /// Rozmiar treści (bez nagłówka) — to ta liczba wchodzi do kontekstu.
    pub fn size(&self) -> usize {
        self.body.len()
    }

    /// `1.2 KB` / `412 B` — jedna miara w modalu i w CLI.
    pub fn size_label(&self) -> String {
        size_label(self.size())
    }

    /// Treść do kontekstu przycięta do [`MAX_CONTENT_BYTES`].
    /// `true` = obcięto (wywołujący dokłada ostrzeżenie).
    pub fn clipped(&self) -> (String, bool) {
        clip(&self.body, MAX_CONTENT_BYTES)
    }
}

pub fn size_label(bytes: usize) -> String {
    if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Przycięcie tekstu do limitu bajtów po granicy znaku.
pub fn clip(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_string(), false);
    }
    let mut cut = max;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (text[..cut].to_string(), true)
}

/// Katalogi w kolejności pierwszeństwa (bez sprawdzania, czy istnieją).
pub fn roots(workspace: &Path) -> Vec<(Source, PathBuf)> {
    vec![
        (Source::Workspace, workspace.join("skills")),
        (Source::App, app_dir().join("skills")),
        (Source::Config, config_dir().join("arndtai").join("skills")),
    ]
}

/// Katalog aplikacji: obok binarki; `ARNDTAI_APP_DIR` pozwala wskazać inny
/// (pakiety, testy) bez zgadywania po `target/`.
pub fn app_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ARNDTAI_APP_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `$XDG_CONFIG_HOME` (albo `~/.config`).
pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    std::env::var("HOME")
        .map(|home| PathBuf::from(home).join(".config"))
        .unwrap_or_else(|_| PathBuf::from(".config"))
}

/// Wszystkie skille, posortowane po nazwie, bez duplikatów `name`
/// (wygrywa pierwszy katalog).
pub fn discover(workspace: &Path) -> Vec<Skill> {
    discover_in(roots(workspace))
}

/// To samo, ale na jawnie podanych katalogach — testy nie dotykają środowiska
/// (config użytkownika, katalog binarki).
pub fn discover_in(dirs: Vec<(Source, PathBuf)>) -> Vec<Skill> {
    let mut found: Vec<Skill> = Vec::new();
    for (source, root) in dirs {
        let entries = match fs::read_dir(&root) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let mut names: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        names.sort();
        for dir in names {
            let file = dir.join("SKILL.md");
            let Ok(text) = fs::read_to_string(&file) else {
                // Brak pliku → katalog pomijany.
                continue;
            };
            let dir_name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("skill")
                .to_string();
            let parsed = parse_document(&text, &dir_name);
            if found.iter().any(|s| s.name == parsed.name) {
                // Pierwszy katalog wygrywa — późniejszy nie nadpisuje.
                continue;
            }
            found.push(Skill {
                name: parsed.name,
                description: parsed.description,
                when: parsed.when,
                path: file,
                source,
                body: parsed.body,
                header_error: parsed.error,
            });
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Skill po nazwie dokładnej (takiej, jaka stoi na liście w promptcie).
pub fn find<'a>(skills: &'a [Skill], name: &str) -> Option<&'a Skill> {
    skills.iter().find(|s| s.name == name)
}

/// Czy nazwa wygląda na ścieżkę — takie `skill.load` odrzuca jak inne bramki.
pub fn is_path_like(name: &str) -> bool {
    name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('\0')
}

/// Lista do promptu systemowego: `- <name>: <description>`, maks. 20 pozycji.
/// Zepsute nagłówki nie wchodzą.
pub fn prompt_lines(skills: &[Skill]) -> Vec<String> {
    skills
        .iter()
        .filter(|s| !s.broken())
        .take(MAX_LIST)
        .map(|s| format!("- {}: {}", s.name, one_line(&s.description)))
        .collect()
}

/// Ile znaków dodaje lista do promptu — liczba do notatki `docs/re/skills.md`.
pub fn list_chars(skills: &[Skill]) -> usize {
    prompt_lines(skills).iter().map(|l| l.chars().count() + 1).sum()
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub struct Parsed {
    pub name: String,
    pub description: String,
    pub when: Option<String>,
    pub body: String,
    pub error: Option<String>,
}

/// Nagłówek `---` + treść. Bez nagłówka: nazwa katalogu i pierwsza linia
/// tekstu. Zepsuty nagłówek nie zatrzymuje odkrywania — wraca jako `error`.
pub fn parse_document(text: &str, dir_name: &str) -> Parsed {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("");
    if first.trim() != "---" {
        return Parsed {
            name: dir_name.to_string(),
            description: first_text_line(text).unwrap_or_else(|| "(brak opisu)".to_string()),
            when: None,
            body: text.to_string(),
            error: None,
        };
    }

    let mut header: Vec<String> = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines {
        if !closed && line.trim() == "---" {
            closed = true;
            continue;
        }
        if closed {
            body_lines.push(line);
        } else {
            header.push(line.to_string());
        }
    }
    let body = body_lines.join("\n");
    if !closed {
        return Parsed {
            name: dir_name.to_string(),
            description: first_text_line(&body).unwrap_or_else(|| "(brak opisu)".to_string()),
            when: None,
            body,
            error: Some("brak zamkniecia naglowka ---".to_string()),
        };
    }

    let mut name = String::new();
    let mut description = String::new();
    let mut when: Option<String> = None;
    for (index, raw) in header.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Parsed {
                name: dir_name.to_string(),
                description: first_text_line(&body).unwrap_or_else(|| "(brak opisu)".to_string()),
                when: None,
                body,
                error: Some(format!(
                    "linia {} naglowka nie jest 'klucz: wartosc'",
                    index + 2
                )),
            };
        };
        let value = unquote(value.trim());
        match key.trim().to_ascii_lowercase().as_str() {
            "name" => name = value,
            "description" => description = value,
            "when" => {
                if !value.is_empty() {
                    when = Some(value);
                }
            }
            // Inne pola YAML nie są błędem — po prostu ich nie znamy.
            _ => {}
        }
    }

    let error = if name.trim().is_empty() {
        Some("brak pola name w naglowku".to_string())
    } else if description.trim().is_empty() {
        Some("brak pola description w naglowku".to_string())
    } else {
        None
    };

    Parsed {
        name: if name.trim().is_empty() {
            dir_name.to_string()
        } else {
            name.trim().to_string()
        },
        description: if description.trim().is_empty() {
            first_text_line(&body).unwrap_or_else(|| "(brak opisu)".to_string())
        } else {
            description.trim().to_string()
        },
        when,
        body,
        error,
    }
}

/// Pierwsza niepusta linia tekstu bez `#`-ów nagłówka markdown.
fn first_text_line(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim().trim_start_matches('#').trim();
        if !trimmed.is_empty() {
            return Some(trimmed.chars().take(160).collect());
        }
    }
    None
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if value.len() >= 2 {
        let first = bytes[0] as char;
        let last = bytes[value.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn temp(tag: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "arndtai-skills-{}-{}-{}",
            tag,
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Tylko katalog workspace'u — testy nie czytają configu maszyny.
    fn dirs(ws: &Path) -> Vec<(Source, PathBuf)> {
        vec![(Source::Workspace, ws.join("skills"))]
    }

    fn put(ws: &Path, name: &str, text: &str) {
        let dir = ws.join("skills").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), text).unwrap();
    }

    const GOOD: &str = "---\nname: raport\ndescription: dzienny raport z postepu\nwhen: gdy trzeba podsumowac dzien\n---\nTresc skilla raport.\n";

    // Katalog aplikacji i config w testach: puste, zeby nie czytać maszyny.
    fn sandbox(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let ws = temp(&format!("{}-ws", tag));
        let app = temp(&format!("{}-app", tag));
        let cfg = temp(&format!("{}-cfg", tag));
        (ws, app, cfg)
    }

    #[test]
    fn two_skills_in_workspace_are_discovered_and_listed() {
        let (ws, _app, _cfg) = sandbox("two");
        put(&ws, "raport", GOOD);
        put(
            &ws,
            "notatki",
            "---\nname: notatki\ndescription: zapisuje notatki\n---\ntresc\n",
        );
        let found = discover_in(dirs(&ws));
        assert_eq!(found.len(), 2, "{:?}", found.iter().map(|s| &s.name).collect::<Vec<_>>());
        assert_eq!(found[0].name, "notatki", "lista sortowana po nazwie");
        assert_eq!(found[1].name, "raport");
        let list = prompt_lines(&found);
        assert_eq!(list.len(), 2);
        assert_eq!(list[1], "- raport: dzienny raport z postepu");
        assert_eq!(found[1].when.as_deref(), Some("gdy trzeba podsumowac dzien"));
        assert!(found[1].body.starts_with("Tresc skilla raport."));
        // Ścieżka i źródło są widoczne w CLI/modalu.
        assert_eq!(found[1].source, Source::Workspace);
        assert!(found[1].path.ends_with("skills/raport/SKILL.md"));
    }

    #[test]
    fn workspace_wins_over_config_for_the_same_name() {
        let ws = temp("prio-ws");
        let app = temp("prio-app");
        let cfg = temp("prio-cfg");
        put(&ws, "raport", GOOD);
        let config_skill = cfg.join("arndtai").join("skills").join("raport");
        fs::create_dir_all(&config_skill).unwrap();
        fs::write(
            config_skill.join("SKILL.md"),
            "---\nname: raport\ndescription: wersja z configu\n---\nconfig\n",
        )
        .unwrap();

        let found = discover_in(vec![
            (Source::Workspace, ws.join("skills")),
            (Source::App, app.join("skills")),
            (Source::Config, cfg.join("arndtai").join("skills")),
        ]);

        assert_eq!(found.len(), 1, "ten sam name nie dubluje sie");
        assert_eq!(found[0].description, "dzienny raport z postepu");
        assert_eq!(found[0].source, Source::Workspace);
        let _ = fs::remove_dir_all(&ws);
        let _ = fs::remove_dir_all(&app);
        let _ = fs::remove_dir_all(&cfg);
    }

    #[test]
    fn broken_yaml_is_listed_but_not_injected() {
        let (ws, _app, _cfg) = sandbox("broken");
        put(&ws, "dobry", GOOD);
        put(&ws, "zepsuty", "---\nname: zepsuty\nto nie jest yaml\n---\ntresc\n");
        let found = discover_in(dirs(&ws));
        assert_eq!(found.len(), 2, "zepsuty skill zostaje na liscie");
        let broken = find(&found, "zepsuty").expect("skill na liscie");
        assert!(broken.broken());
        assert!(
            broken.header_error.as_deref().unwrap_or("").contains("klucz"),
            "{:?}",
            broken.header_error
        );
        let list = prompt_lines(&found);
        assert_eq!(list.len(), 1, "zepsuty nie wchodzi do promptu: {list:?}");
        assert_eq!(list[0], "- raport: dzienny raport z postepu");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn missing_header_uses_directory_name_and_first_line() {
        let (ws, _app, _cfg) = sandbox("nohead");
        put(
            &ws,
            "bez-naglowka",
            "# Moj skill\n\nRob cos pozniej.\nDruga linia.\n",
        );
        let found = discover_in(dirs(&ws));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "bez-naglowka");
        assert_eq!(found[0].description, "Moj skill");
        assert!(found[0].body.contains("Rob cos pozniej."));
        assert!(!found[0].broken());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn directory_without_skill_md_is_skipped() {
        let (ws, _app, _cfg) = sandbox("nofile");
        fs::create_dir_all(ws.join("skills").join("pusty")).unwrap();
        put(&ws, "raport", GOOD);
        let found = discover_in(dirs(&ws));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "raport");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn header_without_description_is_an_error() {
        let parsed = parse_document("---\nname: x\n---\ntresc\n", "katalog");
        assert!(parsed.error.is_some());
        assert_eq!(parsed.name, "x");
        // Brak zamknięcia nagłówka to też błąd, nie panika.
        let open = parse_document("---\nname: x\ndescription: y\n", "katalog");
        assert!(open.error.unwrap_or_default().contains("zamkniecia"));
    }

    #[test]
    fn content_over_8kb_is_clipped_on_a_char_boundary() {
        let body = "ą".repeat(MAX_CONTENT_BYTES); // 2 bajty na znak
        let skill = Skill {
            name: "duzy".to_string(),
            description: "duzy".to_string(),
            when: None,
            path: PathBuf::new(),
            source: Source::Workspace,
            body,
            header_error: None,
        };
        let (text, truncated) = skill.clipped();
        assert!(truncated);
        assert_eq!(text.len(), MAX_CONTENT_BYTES);
        assert!(text.chars().all(|c| c == 'ą'), "granica znaku zachowana");
    }

    #[test]
    fn path_like_names_are_rejected() {
        assert!(is_path_like("../etc/passwd"));
        assert!(is_path_like("a/b"));
        assert!(is_path_like(".."));
        assert!(!is_path_like("raport-dzienny"));
    }

    #[test]
    fn list_is_capped_at_twenty_and_sorted() {
        let (ws, _app, _cfg) = sandbox("cap");
        for i in 0..25 {
            put(
                &ws,
                &format!("s{:02}", i),
                &format!("---\nname: s{:02}\ndescription: opis {}\n---\n", i, i),
            );
        }
        let found = discover_in(dirs(&ws));
        assert_eq!(found.len(), 25, "odkryte wszystkie");
        let list = prompt_lines(&found);
        assert_eq!(list.len(), MAX_LIST, "do promptu wchodzi maks. 20");
        assert_eq!(list[0], "- s00: opis 0", "posortowane po nazwie");
    }
}