//! ArndtAi — agent terminalowy.
//!
//! `mcp` to perimeter wystawiony po stdio (kontrakt docs/contracts/mcp.md v0.1),
//! `run` to pętla agenta, `demo` to zestaw sprawdzeń na realnych artefaktach.

mod agent;
mod clipboard;
mod clock;
mod commands;
mod consolidate;
mod context;
mod markdown;
mod improve;
mod mcp;
mod models;
mod proactive;
mod provider;
mod schema;
mod sessions;
mod settings;
mod shot;
mod skills;
mod store;
mod tools;
mod vault;
mod theme;
mod tui;
mod welcome;

use serde_json::json;
use std::path::{Path, PathBuf};

fn main() {
    std::process::exit(run_cli());
}

fn run_cli() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // `ARNDTAI_TIMEOUT` walidujemy przed czymkolwiek: zla wartosc to blad
    // klienta (exit 2), a nie ciche 300 s albo ubity przebieg po 120 s.
    if let Err(message) = provider::timeout_from_env() {
        eprintln!("{}", message);
        return 2;
    }
    // Zrzut do PNG: tryb bezobslugowy, konczy sie po zapisaniu pliku.
    // `--screenshot-welcome` rysuje ekran powitalny, `--screenshot-resume`
    // modal sesji, `--screenshot-list` liste sesji, `--screenshot` ekran
    // sesji — domyslnie 120x38 (960x608), z opcjonalnym rozmiarem `83x38`,
    // bez dotykania terminala.
    let (cols, rows) = size_arg(&args).unwrap_or((shot::DEFAULT_COLS, shot::DEFAULT_ROWS));
    for (flag, saver) in [
        (
            "--screenshot-welcome",
            shot::save_welcome_at as fn(&str, u16, u16) -> i32,
        ),
        ("--screenshot-resume", shot::save_resume_at),
        ("--screenshot-model", shot::save_model_at),
        ("--screenshot-hints", shot::save_hints_at),
        ("--screenshot-settings", shot::save_settings_at),
        ("--screenshot-ask", shot::save_ask_at),
        ("--screenshot-light", shot::save_light_at),
        ("--screenshot-skills", shot::save_skills_at),
        ("--screenshot-skill", shot::save_skill_at),
        ("--screenshot-search", shot::save_search_at),
        ("--screenshot-copy", shot::save_copy_at),
        ("--screenshot-export", shot::save_export_at),
        ("--screenshot-markdown", shot::save_markdown_at),
        ("--screenshot-markdown-40", shot::save_markdown_40_at),
        ("--screenshot-stream", shot::save_stream_at),
    ] {
        if args
            .iter()
            .any(|a| a == flag || a.starts_with(&format!("{}=", flag)))
        {
            return match flag_arg(&args, flag) {
                Some(path) => saver(&path, cols, rows),
                None => {
                    eprintln!("{} wymaga sciezki pliku .png", flag);
                    2
                }
            };
        }
    }
    if args.iter().any(|a| a == "--screenshot-list" || a.starts_with("--screenshot-list=")) {
        return match screenshot_list_arg(&args) {
            Some(path) => shot::save_list_at(&path, cols, rows),
            None => {
                eprintln!("--screenshot-list wymaga sciezki pliku .png");
                2
            }
        };
    }
    if args
        .iter()
        .any(|a| a == "--screenshot" || a.starts_with("--screenshot="))
    {
        return match screenshot_arg(&args) {
            Some(path) => shot::save_at(&path, cols, rows),
            None => {
                eprintln!("--screenshot wymaga sciezki pliku .png");
                2
            }
        };
    }
    if args.is_empty() {
        usage();
        return 0;
    }
    match args[0].as_str() {
        "help" | "-h" | "--help" => {
            usage();
            0
        }
        // Wersja musi byc flaga, nie trescia zadania: skrypty instalacyjne
        // sprawdzaja `ardntai --version` przed uruchomieniem.
        "version" | "-V" | "--version" => {
            println!("ardntai {}", env!("CARGO_PKG_VERSION"));
            0
        }
        // `ardntai tui` oraz `ardntai --tui` — jeden tryb, dwie pisownie.
        "tui" | "--tui" => match tui::parse(&args[1..]) {
            Ok(opts) => tui::run(opts),
            Err(code) => code,
        },
        "mcp" => serve_mcp(&args[1..]),
        "tools" => {
            list_tools();
            0
        }
        "improve" => {
            let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let proposals = improve::propose(&ws);
            if proposals.is_empty() {
                println!("brak propozycji (za malo powtarzalnych sygnalow)");
                return 0;
            }
            for proposal in &proposals {
                println!("== {} [{}]", proposal.title, proposal.kind);
                println!("{}", proposal.body);
            }
            if args.iter().skip(1).any(|a| a == "--apply") {
                match improve::apply(&ws, &proposals, context::now_ts()) {
                    Ok(saved) => {
                        for path in saved {
                            println!("zapisane: {}", path.display());
                        }
                        0
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        1
                    }
                }
            } else {
                println!("(podglad; --apply zapisze do docs/proposals, bez zmiany policies/)");
                0
            }
        }
        "ingest" => {
            let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let mut source: Option<String> = None;
            let mut title: Option<String> = None;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--title" => {
                        title = args.get(i + 1).cloned();
                        i += 2;
                    }
                    other if !other.starts_with('-') && source.is_none() => {
                        source = Some(other.to_string());
                        i += 1;
                    }
                    other => {
                        eprintln!("ingest: nieznany argument {other}");
                        return 2;
                    }
                }
            }
            let Some(source) = source else {
                eprintln!("ingest: podaj plik, np. arndtai ingest nota.md --title \"Nota\"");
                return 2;
            };
            let title = title.unwrap_or_else(|| {
                Path::new(&source)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("notatka")
                    .to_string()
            });
            match vault::ingest(&ws, &source, &title) {
                Ok(saved) => {
                    println!("zapisane: {saved}");
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            }
        }
        "consolidate" => {
            let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match consolidate::consolidate(&ws, context::now_ts()) {
                Ok(report) => {
                    println!(
                        "fakty {} · epizody {} · klastry {}",
                        report.facts, report.episodes, report.clusters
                    );
                    match report.graph {
                        Some(path) => println!("graf: {}", path.display()),
                        None => println!("brak danych — nic nie zapisano"),
                    }
                    0
                }
                Err(error) => {
                    eprintln!("{error}");
                    1
                }
            }
        }
        "tick" => {
            let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let now = context::now_ts();
            let run_actions = args.iter().skip(1).any(|a| a == "--run");
            let hits = if run_actions {
                let binary = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ardntai"));
                proactive::tick_run(&ws, now, &binary)
            } else {
                proactive::tick(&ws, now)
            };
            for hit in &hits {
                println!(
                    "{} {} — {}",
                    if hit.shadow { "shadow" } else { "notatka" },
                    hit.policy,
                    hit.effect
                );
            }
            if hits.is_empty() {
                println!("brak trafien (default-deny)");
            }
            0
        }
        "skills" => match memory_arg(&args[1..], "skills") {
            Ok(level) => {
                list_skills(level);
                0
            }
            Err(code) => code,
        },
        "status" => match memory_arg(&args[1..], "status") {
            Ok(level) => {
                status(level);
                0
            }
            Err(code) => code,
        },
        "demo" => demo(),
        "run" => {
            if args.len() < 2 {
                usage();
                return 2;
            }
            run_task(&args[1..])
        }
        _ => run_task(&args),
    }
}

fn screenshot_arg(args: &[String]) -> Option<String> {
    flag_arg(args, "--screenshot")
}

/// Opcjonalny rozmiar kadru w znakach: `83x38` (kolumny × wiersze). Pozwala
/// porównać ekran linia po linii ze zrzutem referencyjnym o innej szerokości
/// niż domyślne 120×38. Token musi być w całości `LICZBAxLICZBA`.
fn size_arg(args: &[String]) -> Option<(u16, u16)> {
    for arg in args {
        let Some((w, h)) = arg.split_once('x').or_else(|| arg.split_once('X')) else {
            continue;
        };
        if w.is_empty()
            || h.is_empty()
            || !w.bytes().all(|b| b.is_ascii_digit())
            || !h.bytes().all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let (Ok(cols), Ok(rows)) = (w.parse::<u16>(), h.parse::<u16>()) else {
            continue;
        };
        if (20..=500).contains(&cols) && (6..=200).contains(&rows) {
            return Some((cols, rows));
        }
    }
    None
}

/// Wartość flagi w obu pisowniach: `--flaga plik.png` i `--flaga=plik.png`.
fn flag_arg(args: &[String], flag: &str) -> Option<String> {
    let prefix = format!("{}=", flag);
    for (i, arg) in args.iter().enumerate() {
        if arg == flag {
            return args.get(i + 1).cloned();
        }
        if let Some(path) = arg.strip_prefix(&prefix) {
            return Some(path.to_string());
        }
    }
    None
}

fn screenshot_list_arg(args: &[String]) -> Option<String> {
    flag_arg(args, "--screenshot-list")
}

/// F23: poziom pamieci dla komend pomocniczych — ta sama flaga i te same
/// wartosci co w `run` (`--memory lite|pro|ultra` oraz `--memory=pro`).
/// Domyslnie `lite`, tak jak dotad.
fn memory_arg(args: &[String], command: &str) -> Result<context::MemoryLevel, i32> {
    let mut level = context::MemoryLevel::Lite;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].as_str();
        let raw = match arg {
            "--memory" => match args.get(i + 1) {
                Some(v) => {
                    i += 2;
                    v.as_str()
                }
                None => {
                    eprintln!("--memory wymaga poziomu (lite|pro|ultra)");
                    return Err(2);
                }
            },
            other if other.starts_with("--memory=") => {
                i += 1;
                &other["--memory=".len()..]
            }
            other => {
                eprintln!("nieznany argument {}: {}", command, other);
                return Err(2);
            }
        };
        match context::MemoryLevel::parse(raw) {
            Some(parsed) => level = parsed,
            None => {
                eprintln!("--memory: dozwolone lite, pro, ultra");
                return Err(2);
            }
        }
    }
    Ok(level)
}

fn usage() {
    println!("ArndtAi 0.1 — agent terminalowy");
    println!();
    println!("  arndtai run \"zadanie\" [--provider mock|anthropic|openai] [--workspace FOLDER]");
    println!("                [--steps N] [--memory lite|pro|ultra] [--mcp|--no-mcp] [-q]");
    println!("  arndtai tui [\"zadanie\"] [--provider ...] [--workspace FOLDER] [--steps N]");
    println!("                      — ekran TUI: scrollback eventow, prompt, pasek statusu");
    println!("                      bez zadania: ekran powitalny (ctrl+r sesje, ctrl+w lista)");
    println!("  arndtai demo      — sprawdzenia: petla z narzedziami, scope guard, allowlista,");
    println!("                      MCP, kontrakt narzedzi, dryf, poziomy pamieci");
    println!("  arndtai mcp [--workspace FOLDER]   — serwer MCP po stdio (perimeter)");
    println!("  arndtai tools     — lista narzedzi perimeteru");
    println!("  arndtai skills [--memory lite|pro|ultra]  — osobowosc, skille i pamiec projektu");
    println!("  arndtai tick [--run]                       — reguly proaktywne z policies/*.json");
    println!("  arndtai consolidate                        — graf pamieci z faktow i epizodow");
    println!("  arndtai ingest <plik> [--title \"...\"]      — second brain: kopia do .agent-memory/vault");
    println!("  arndtai improve [--apply]                  — propozycje regul z powtarzalnych odmow");
    println!("  arndtai status [--memory lite|pro|ultra]  — stan workspace'u (sesje, mozg, outbox)");
    println!("  arndtai --screenshot plik.png [WxH]   — zrzut ekranu sesji (960x608 = 120x38)");
    println!("  arndtai --screenshot-welcome plik.png — zrzut ekranu powitalnego");
    println!("  arndtai --screenshot-resume plik.png  — zrzut modala „Resume session\"");
    println!("  arndtai --screenshot-model plik.png   — zrzut modala „Select model\"");
    println!("  arndtai --screenshot-hints plik.png   — zrzut podpowiedzi komend `/`");
    println!("  arndtai --screenshot-settings plik.png — zrzut modala `/settings`");
    println!("  arndtai --screenshot-ask plik.png     — zrzut pytania trybu `ask`");
    println!("  arndtai --screenshot-skills plik.png  — zrzut modala `/skills`");
    println!("  arndtai --screenshot-skill plik.png   — zrzut scrollbacku po `/skill`");
    println!("  arndtai --screenshot-search plik.png  — zrzut modala szukania (Ctrl+F)");
    println!("  arndtai --screenshot-copy plik.png    — zrzut scrollbacku po `/copy`");
    println!("  arndtai --screenshot-export plik.png  — zrzut scrollbacku po `/export`");
    println!("  arndtai --screenshot-light plik.png   — zrzut motywu `jasny`");
    println!("  arndtai --screenshot-markdown plik.png — zrzut odpowiedzi w markdownie");
    println!("  arndtai --screenshot-markdown-40 plik.png 40x38 — ten sam ekran w 40 kolumnach");
    println!("  arndtai --screenshot-stream plik.png  — blok myśli w trakcie strumienia");
    println!("  arndtai --screenshot-list plik.png    — zrzut listy sesji");
    println!("                [WxH] np. 83x38 — kadr do porownania ze zrzutem referencyjnym");
    println!();
    println!("TRYB PRACY");
    println!("  offline (mock): \"read plik; write plik :: tresc; append plik :: dopisek; notify komunikat\"");
    println!("  zdalny: ANTHROPIC_API_KEY albo OPENAI_API_KEY (opcjonalnie ARNDTAI_MODEL,");
    println!("          ARNDTAI_BASE_URL, ARNDTAI_TIMEOUT w sekundach — domyslnie 300); wybor przez --provider");
    println!("  transkrypty: FOLDER/.agent/sessions/");
}

fn serve_mcp(args: &[String]) -> i32 {
    let mut workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" | "--workspace" => match args.get(i + 1) {
                Some(v) => {
                    workspace = PathBuf::from(v);
                    i += 2;
                }
                None => {
                    eprintln!("--workspace wymaga sciezki");
                    return 2;
                }
            },
            other => {
                eprintln!("nieznany argument mcp: {}", other);
                return 2;
            }
        }
    }
    mcp::serve(workspace)
}

/// Wskaznik ostatniej sesji w `.agent/resume` — jedyny sposob, by po restarcie
/// trafic w ten sam `session_id` (inaczej idzie nowy, a `steps` zostaje puste).
pub fn read_resume_pointer(workspace: &Path) -> Option<String> {
    let path = workspace.join(".agent").join("resume");
    let raw = std::fs::read_to_string(path).ok()?;
    let id = raw.trim();
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub fn write_resume_pointer(workspace: &Path, session_id: &str) {
    let dir = workspace.join(".agent");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::write(dir.join("resume"), session_id);
}

fn run_task(args: &[String]) -> i32 {
    let mut workspace = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut provider: Option<String> = None;
    let mut max_steps: usize = 12;
    let mut quiet = false;
    let mut use_mcp = true;
    let mut resume = false;
    let mut memory = context::MemoryLevel::Lite;
    // Ekstrakcja faktow: domyslnie tylko `ultra`; `--facts` wlacza ja tez na `pro`.
    let mut facts_flag = false;
    let mut facts_set = false;
    let mut task_parts: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" | "--workspace" => {
                if let Some(v) = args.get(i + 1) {
                    workspace = PathBuf::from(v);
                    i += 2;
                } else {
                    eprintln!("--workspace wymaga sciezki");
                    return 2;
                }
            }
            "--provider" => {
                if let Some(v) = args.get(i + 1) {
                    provider = Some(v.clone());
                    i += 2;
                } else {
                    eprintln!("--provider wymaga nazwy");
                    return 2;
                }
            }
            "--steps" => {
                if let Some(v) = args.get(i + 1) {
                    match v.parse::<usize>() {
                        Ok(n) if n > 0 => max_steps = n,
                        _ => {
                            eprintln!("--steps wymaga liczby > 0");
                            return 2;
                        }
                    }
                    i += 2;
                } else {
                    eprintln!("--steps wymaga liczby");
                    return 2;
                }
            }
            "--facts" => {
                facts_flag = true;
                facts_set = true;
                i += 1;
            }
            "--no-facts" => {
                facts_flag = false;
                facts_set = true;
                i += 1;
            }
            "--memory" => {
                if let Some(v) = args.get(i + 1) {
                    match context::MemoryLevel::parse(v) {
                        Some(level) => memory = level,
                        None => {
                            eprintln!("--memory: dozwolone lite, pro, ultra");
                            return 2;
                        }
                    }
                    i += 2;
                } else {
                    eprintln!("--memory wymaga poziomu (lite|pro|ultra)");
                    return 2;
                }
            }
            "--mcp" => {
                use_mcp = true;
                i += 1;
            }
            "--no-mcp" => {
                use_mcp = false;
                i += 1;
            }
            "--resume" => {
                resume = true;
                i += 1;
            }
            "-q" | "--quiet" => {
                quiet = true;
                i += 1;
            }
            other => {
                task_parts.push(other.to_string());
                i += 1;
            }
        }
    }

    let task = task_parts.join(" ");
    if task.trim().is_empty() {
        usage();
        return 2;
    }

    // Runda 15: limit odpowiedzi z `.agent/config` (`max_tokens`), przycięty
    // do `limit.output` modelu z cache. Brak configu = 4096 (bez zmian).
    let settings = settings::Settings::load(&workspace);
    let model_id = models::env_model()
        .or_else(|| models::config_get(&workspace, "model"))
        .unwrap_or_default();
    let model_limit = if model_id.is_empty() {
        None
    } else {
        models::read_cache(&workspace)
            .and_then(|catalog| catalog.find(&model_id).and_then(|m| m.output))
    };
    let max_tokens = Some(settings.token_budget(model_limit));

    let facts_provider = provider.clone();
    let facts_workspace = workspace.clone();
    let cfg = agent::Config {
        workspace,
        max_steps,
        allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
        provider,
        quiet,
        mcp: use_mcp,
        memory,
        resume,
        model: None,
        max_tokens,
        approver: None,
        skill_context: Vec::new(),
    };
    let report = agent::run(&task, &cfg);
    // Fakty po zadaniu: domyslnie na `ultra`, na `pro` tylko z `--facts`.
    let want_facts = if facts_set {
        facts_flag
    } else {
        matches!(memory, context::MemoryLevel::Ultra)
    };
    if want_facts && report.status == "DONE" {
        if let Ok(facts) =
            provider::extract_facts(facts_provider.as_deref(), &task, &report.reason)
        {
            append_facts(&facts_workspace, &facts);
        }
    }
    match report.status.as_str() {
        "DONE" => 0,
        // Osobny kod: "ktos inny pracuje na tym workspace" to nie blad zadania.
        "FAILED_LOCKED" => 2,
        _ => 1,
    }
}

/// Dopisuje fakty do `.agent-memory/facts.md`, pomijajac duplikaty.
/// Plik jest czytany przez pamiec `pro`/`ultra` w kolejnych przebiegach.
pub(crate) fn append_facts(workspace: &Path, facts: &[String]) {
    if facts.is_empty() {
        return;
    }
    let dir = workspace.join(".agent-memory");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join("facts.md");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut out = existing.clone();
    if existing.trim().is_empty() {
        out.push_str("# Fakty (poziom pro/ultra)\n\n");
    }
    let mut added = 0;
    for fact in facts {
        let line = format!("- {}\n", fact.trim());
        if out.contains(line.trim_end()) {
            continue;
        }
        out.push_str(&line);
        added += 1;
    }
    if added > 0 {
        let _ = std::fs::write(&path, out);
    }
}

fn list_tools() {
    println!("Narzedzia perimeteru ({}):", tools::NAMES.len());
    for (name, example, desc) in tools::describe() {
        println!("  {:<12} {}", name, desc);
        println!("               args: {}", example);
    }
}

/// Runda 16: ta sama lista co w modalu `/skills`, z katalogiem źródłowym
/// i znacznikiem zepsutego nagłówka. Format jest stabilny — czyta go człowiek
/// i bramka G17.
fn list_skills(level: context::MemoryLevel) {
    let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let prefix = context::Prefix::load(&ws, level);
    println!("Folder:     {}", ws.display());
    println!("Osobowosc:  {}", prefix.persona_source);
    println!("Poziom pamieci: {}", prefix.memory_level.as_str());
    if prefix.skills.is_empty() {
        println!(
            "Skille:     brak (szukam w {}/skills/<nazwa>/SKILL.md)",
            ws.display()
        );
        println!("            potem: {} i ~/.config/arndtai/skills", skills::app_dir().join("skills").display());
    } else {
        println!("Skille ({}):", prefix.skills.len());
        for skill in &prefix.skills {
            let mark = if skill.broken() { "  [blad naglowka]" } else { "" };
            println!(
                "  {:<20} {:<8} {}{}",
                skill.name,
                skill.size_label(),
                skill.description,
                mark
            );
            println!(
                "  {:<20} zrodlo: {}  plik: {}",
                "",
                skill.source.label(),
                skill.path.display()
            );
            if let Some(when) = &skill.when {
                println!("  {:<20} kiedy: {}", "", when);
            }
        }
        let list = skills::prompt_lines(&prefix.skills);
        println!(
            "Lista w promptcie: {} pozycji ({} znakow), tresc: /skill <nazwa> albo skill.load",
            list.len(),
            skills::list_chars(&prefix.skills)
        );
    }
    println!(
        "Pamiec:     {}",
        prefix.memory_source.as_deref().unwrap_or("brak")
    );
}

fn status(level: context::MemoryLevel) {
    let ws = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let prefix = context::Prefix::load(&ws, level);
    println!("Folder:     {}", ws.display());
    println!("Osobowosc:  {} ({} znakow)", prefix.persona_source, prefix.persona.len());
    println!("Poziom pamieci: {}", prefix.memory_level.as_str());
    match &prefix.memory_source {
        Some(src) => println!("Pamiec:     {}", src),
        None => println!("Pamiec:     brak pliku pamieci"),
    }
    println!("Skille:     {}", prefix.skills.len());
    for skill in &prefix.skills {
        println!("  - {}", skill.name);
    }

    let files = context::sessions(&ws);
    println!("Sesje:      {}", files.len());
    if let Some(last) = files.last() {
        let name = last
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        println!("Ostatnia:   {}", name);
        if let Ok(text) = std::fs::read_to_string(last) {
            if let Some(line) = text.lines().last() {
                println!("  {}", first_chars(line, 200));
            }
        }
    }

    let brain = provider::Brain::from_env(None, "", &[]);
    println!("Mozg:       {}", brain.label());
    if let Some(note) = brain.note() {
        println!("Uwaga:      {}", note);
    }

    let outbox = ws.join(".agent").join("outbox").join("notifications.log");
    match std::fs::read_to_string(&outbox) {
        Ok(text) => println!("Outbox:     {} linii", text.lines().count()),
        Err(_) => println!("Outbox:     pusty"),
    }
}

fn first_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn check(results: &mut Vec<(String, bool)>, name: impl Into<String>, ok: bool) {
    results.push((name.into(), ok));
}

fn demo() -> i32 {
    let base = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".agent")
        .join("demo");
    let _ = std::fs::remove_dir_all(&base);

    println!("== DEMO ArndtAi ==");
    let mut results: Vec<(String, bool)> = Vec::new();

    // --- 1. petla z narzedziami przez MCP ---
    let ws1 = base.join("run1");
    let _ = std::fs::create_dir_all(&ws1);
    let cfg1 = agent::Config {
        workspace: ws1.clone(),
        max_steps: 12,
        allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
        provider: Some("mock".to_string()),
        quiet: true,
        mcp: true,
        memory: context::MemoryLevel::Lite,
        resume: false,
        model: None,
        max_tokens: None,
        approver: None,
        skill_context: Vec::new(),
    };
    let task1 = "write notatka.txt :: ArndtAi dziala; read notatka.txt; notify notatka gotowa";
    let r1 = agent::run(task1, &cfg1);
    let notatka = ws1.join("notatka.txt");
    let outbox1 = ws1.join(".agent").join("outbox").join("notifications.log");
    check(&mut results, "run1: plik zapisany", notatka.is_file());
    check(
        &mut results,
        "run1: tresc pliku",
        read_text(&notatka).contains("dziala"),
    );
    check(&mut results, "run1: 3 wywolania narzedzi", r1.tool_calls == 3);
    check(
        &mut results,
        "run1: powiadomienie w outboxie",
        read_text(&outbox1).contains("notatka gotowa"),
    );
    check(&mut results, "run1: status DONE", r1.status == "DONE");
    check(&mut results, "run1: kroki policzone", r1.steps >= 3);
    check(&mut results, "run1: powod wypelniony", !r1.reason.is_empty());
    check(&mut results, "run1: transkrypt sesji", r1.session.is_file());

    // --- 2. scope guard ---
    let ws2 = base.join("run2");
    let _ = std::fs::create_dir_all(&ws2);
    let cfg2 = agent::Config {
        workspace: ws2.clone(),
        max_steps: 4,
        allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
        provider: Some("mock".to_string()),
        quiet: true,
        mcp: true,
        memory: context::MemoryLevel::Lite,
        resume: false,
        model: None,
        max_tokens: None,
        approver: None,
        skill_context: Vec::new(),
    };
    let r2 = agent::run("write ../escape.txt :: nie powinno powstac", &cfg2);
    check(&mut results, "run2: odmowa scope guardu", r2.denied >= 1);
    check(
        &mut results,
        "run2: plik poza workspace nie powstaje",
        !base.join("escape.txt").exists(),
    );
    check(&mut results, "run2: sesja konczy sie czysto", r2.status == "DONE");

    // --- 3. allowlista narzedzi ---
    let ws3 = base.join("run3");
    let _ = std::fs::create_dir_all(&ws3);
    let cfg3 = agent::Config {
        workspace: ws3.clone(),
        max_steps: 3,
        allowlist: vec!["fs.read".to_string()],
        provider: Some("mock".to_string()),
        quiet: true,
        mcp: false,
        memory: context::MemoryLevel::Lite,
        resume: false,
        model: None,
        max_tokens: None,
        approver: None,
        skill_context: Vec::new(),
    };
    let r3 = agent::run("notify test allowlisty", &cfg3);
    let outbox3 = ws3.join(".agent").join("outbox").join("notifications.log");
    check(&mut results, "run3: odmowa allowlisty", r3.denied >= 1);
    check(
        &mut results,
        "run3: notify nie trafia do outboxa",
        !outbox3.exists(),
    );
    check(&mut results, "run3: sesja konczy sie czysto", r3.status == "DONE");

    // --- 4. MCP bezposrednio: handshake, tools/list, tools/call ---
    let ws4 = base.join("run4");
    let _ = std::fs::create_dir_all(&ws4);
    match mcp::McpClient::spawn(&ws4) {
        Ok(mut client) => {
            let listed = client.list_tools().unwrap_or_default();
            check(
                &mut results,
                &format!("mcp: tools/list zwraca {} narzedzia", tools::NAMES.len()),
                listed.len() == tools::NAMES.len(),
            );
            // Kontrakt mcp.md: kazde wywolanie niesie wlasny `step_id`.
            let ctxw = tools::CallCtx {
                task_id: "demo".to_string(),
                session_id: "demo".to_string(),
                step_id: 1,
            };
            let wrote = client.call(
                "fs.write",
                &json!({"path": "mcp.txt", "content": "przez mcp"}),
                &ctxw,
            );
            check(
                &mut results,
                "mcp: fs.write zadzialal",
                matches!(&wrote, Ok(r) if r.ok),
            );
            check(
                &mut results,
                "mcp: plik z fs.write istnieje",
                read_text(&ws4.join("mcp.txt")) == "przez mcp",
            );
            let ctx_read = tools::CallCtx { step_id: 2, ..ctxw.clone() };
            let read = client.call("fs.read", &json!({"path": "mcp.txt"}), &ctx_read);
            check(
                &mut results,
                "mcp: fs.read zwraca tresc",
                matches!(
                    &read,
                    Ok(r) if r.ok
                        && r.output.get("content").and_then(|v| v.as_str()) == Some("przez mcp")
                ),
            );
            let ctx_notify = tools::CallCtx { step_id: 3, ..ctxw.clone() };
            let notified = client.call("notify.send", &json!({"message": "mcp ok"}), &ctx_notify);
            check(
                &mut results,
                "mcp: notify.send zwraca status queued",
                matches!(
                    &notified,
                    Ok(r) if r.ok
                        && r.output.get("status").and_then(|v| v.as_str()) == Some("queued")
                ),
            );
            client.shutdown();
        }
        Err(e) => {
            eprintln!("  mcp nie wystartowal: {}", e);
            check(&mut results, "mcp: serwer wystartowal", false);
        }
    }

    // --- 5. kontrakt narzedzi: kody bledow i zapis atomowy ---
    let ws5 = base.join("run5");
    let _ = std::fs::create_dir_all(&ws5);
    let code_of = |name: &str, args: serde_json::Value| -> Option<String> {
        tools::call(name, &args, &ws5).code
    };
    check(
        &mut results,
        "kontrakt: '..' daje DENIED",
        code_of("fs.read", json!({"path": "../tajne.txt"})).as_deref() == Some("DENIED"),
    );
    check(
        &mut results,
        "kontrakt: sciezka absolutna daje DENIED",
        code_of("fs.read", json!({"path": "/etc/passwd"})).as_deref() == Some("DENIED"),
    );
    check(
        &mut results,
        "kontrakt: pusta sciezka daje SCHEMA_ERROR",
        code_of("fs.read", json!({"path": ""})).as_deref() == Some("SCHEMA_ERROR"),
    );
    let big = "a".repeat(tools::MAX_WRITE_BYTES + 1);
    check(
        &mut results,
        "kontrakt: zapis ponad 1 MB daje DENIED",
        code_of("fs.write", json!({"path": "duzy.txt", "content": big})).as_deref()
            == Some("DENIED"),
    );
    check(
        &mut results,
        "kontrakt: plik ponad limit nie powstaje",
        !ws5.join("duzy.txt").exists(),
    );
    check(
        &mut results,
        "kontrakt: brak pliku daje NOT_FOUND",
        code_of("fs.read", json!({"path": "nie_ma.txt"})).as_deref() == Some("NOT_FOUND"),
    );
    check(
        &mut results,
        "kontrakt: fs.write bez content daje SCHEMA_ERROR",
        code_of("fs.write", json!({"path": "x.txt"})).as_deref() == Some("SCHEMA_ERROR"),
    );
    check(
        &mut results,
        "kontrakt: nieznane narzedzie daje DENIED",
        code_of("shell.exec", json!({"cmd": "ls"})).as_deref() == Some("DENIED"),
    );
    let atom = tools::call(
        "fs.write",
        &json!({"path": "atom.txt", "content": "atom"}),
        &ws5,
    );
    let leftovers: Vec<String> = std::fs::read_dir(&ws5)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(".tmp"))
                .collect()
        })
        .unwrap_or_default();
    check(
        &mut results,
        "kontrakt: zapis atomowy nie zostawia .tmp",
        atom.ok && leftovers.is_empty(),
    );
    let default_channel = tools::call(
        "notify.send",
        &json!({"message": "bez kanalu"}),
        &ws5,
    );
    check(
        &mut results,
        "kontrakt: notify bez kanalu uzywa 'default'",
        default_channel.ok
            && default_channel.output.get("channel").and_then(|v| v.as_str()) == Some("default"),
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let link = ws5.join("link");
        let made = symlink("/etc", &link).is_ok();
        check(&mut results, "kontrakt: symlink utworzony", made);
        if made {
            check(
                &mut results,
                "kontrakt: symlink poza workspace daje DENIED",
                code_of("fs.read", json!({"path": "link/passwd"})).as_deref() == Some("DENIED"),
            );
        }
    }

    // --- 6. detektor dryfu ---
    let ws6 = base.join("run6");
    let _ = std::fs::create_dir_all(&ws6);
    let cfg6 = agent::Config {
        workspace: ws6.clone(),
        max_steps: 6,
        allowlist: tools::NAMES.iter().map(|n| n.to_string()).collect(),
        provider: Some("mock".to_string()),
        quiet: true,
        mcp: false,
        memory: context::MemoryLevel::Lite,
        resume: false,
        model: None,
        max_tokens: None,
        approver: None,
        skill_context: Vec::new(),
    };
    // Runda 9: dryf liczy sie tylko dla mutacji — ten sam zapis dwa razy.
    let r6 = agent::run("write dryf.txt :: x; write dryf.txt :: x", &cfg6);
    check(&mut results, "dryf: powtorzenie wykryte", r6.drift == 1);
    check(
        &mut results,
        "dryf: drugie wywolanie nie poszlo",
        r6.tool_calls == 1,
    );

    // --- 7. poziomy pamieci i osobowosc z pliku ---
    let ws7 = base.join("run7");
    let _ = std::fs::create_dir_all(ws7.join("personality"));
    let _ = std::fs::create_dir_all(ws7.join("memory"));
    let _ = std::fs::write(ws7.join("personality").join("base.md"), "Jestem testerem.");
    let _ = std::fs::write(ws7.join("memory").join("project.md"), "Projekt testowy.");
    let _ = std::fs::write(ws7.join("memory").join("facts.md"), "Fakt: 1.");
    let lite = context::Prefix::load(&ws7, context::MemoryLevel::Lite);
    let pro = context::Prefix::load(&ws7, context::MemoryLevel::Pro);
    check(
        &mut results,
        "pamiec: osobowosc z personality/base.md",
        lite.persona_source == "personality/base.md",
    );
    check(
        &mut results,
        "pamiec: lite widzi osobowosc",
        lite.render().contains("Jestem testerem."),
    );
    check(
        &mut results,
        "pamiec: lite widzi project.md",
        lite.render().contains("Projekt testowy."),
    );
    check(
        &mut results,
        "pamiec: lite nie widzi facts.md",
        !lite.render().contains("Fakt: 1."),
    );
    check(
        &mut results,
        "pamiec: pro widzi facts.md",
        pro.render().contains("Fakt: 1."),
    );
    check(
        &mut results,
        "pamiec: nazwy poziomow",
        context::MemoryLevel::Lite.as_str() == "lite"
            && context::MemoryLevel::Ultra.as_str() == "ultra",
    );

    let passed = results.iter().filter(|(_, ok)| *ok).count();
    for (name, ok) in &results {
        println!("  {} {}", if *ok { "OK  " } else { "FAIL" }, name);
    }
    println!("{}/{} sprawdzen przeszlo", passed, results.len());
    if passed == results.len() {
        println!("DEMO OK");
        0
    } else {
        println!("DEMO FAIL");
        1
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn size_arg_takes_only_a_whole_wxh_token() {
        assert_eq!(
            size_arg(&args(&["--screenshot-welcome", "x.png", "83x38"])),
            Some((83, 38))
        );
        assert_eq!(size_arg(&args(&["--screenshot", "shots/now.png"])), None);
        assert_eq!(size_arg(&args(&["--screenshot", "w83x38.png"])), None);
        assert_eq!(size_arg(&args(&["4x4"])), None, "za mały kadr odrzucony");
    }
}
