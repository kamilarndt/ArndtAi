//! Narzędzia perimeteru. Kontrakt: docs/contracts/mcp.md v0.1.
//!
//! Każde wywołanie działa wyłącznie w obrębie workspace'u. Wynik jest
//! strukturalnym JSON-em, a błąd niesie jeden z kodów kontraktu:
//! DENIED, NOT_FOUND, IO_ERROR, TIMEOUT, SCHEMA_ERROR.

use serde_json::{json, Value};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CONTRACT_VERSION: &str = "0.1";
pub const MAX_WRITE_BYTES: usize = 1_048_576;
pub const MAX_READ_BYTES: usize = 4_194_304;

/// Runda 16: `skill.load` jest read-only jak `fs.read` — czyta odkryte skille
/// z workspace'u i nie dotyka dysku poza odczytem `SKILL.md`.
pub const NAMES: [&str; 5] = ["fs.read", "fs.write", "fs.append", "notify.send", "skill.load"];

/// Narzedzia zmieniajace stan. Detektor dryfu liczy tylko je — `fs.read`
/// jest read-only, wiec nie wchodzi do okna i nie moze wywolac odmowy.
pub fn is_mutating(name: &str) -> bool {
    matches!(name, "fs.write" | "fs.append" | "notify.send")
}

pub fn describe() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            "fs.read",
            "{\"path\":\"wzgledna/sciezka\"}",
            "czyta plik tekstowy z workspace'u i zwraca {content, size}",
        ),
        (
            "fs.write",
            "{\"path\":\"wzgledna/sciezka\",\"content\":\"tresc\"}",
            "zapisuje plik atomowo w workspace'ie (limit 1 MB, tworzy katalogi)",
        ),
        (
            "fs.append",
            "{\"path\":\"wzgledna/sciezka\",\"content\":\"dopisek\"}",
            "dopisuje tresc na koncu pliku w workspace (tworzy go, limit 1 MB) i zwraca {bytes_written}",
        ),
        (
            "notify.send",
            "{\"channel\":\"default\",\"message\":\"komunikat\"}",
            "odkłada powiadomienie do outboxa i zwraca {status: queued}",
        ),
        (
            "skill.load",
            "{\"name\":\"raport-dzienny\"}",
            "zwraca tresc skilla z listy ({content, bytes}, limit 8 KB) albo NOT_FOUND",
        ),
    ]
}

/// JSON Schema wejścia narzędzia — to samo, co serwer wystawia w `tools/list`.
pub fn input_schema(name: &str) -> Option<Value> {
    match name {
        "fs.read" => Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "sciezka wzgledna w workspace" }
            },
            "required": ["path"],
            "additionalProperties": false
        })),
        "fs.write" => Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "sciezka wzgledna w workspace" },
                "content": { "type": "string", "description": "tresc pliku (do 64 KB; twardy limit kontraktu 1 MB)" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        })),
        "fs.append" => Some(json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "sciezka wzgledna w workspace" },
                "content": { "type": "string", "description": "tresc dopisywana na koncu pliku (do 64 KB; twardy limit kontraktu 1 MB)" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        })),
        "notify.send" => Some(json!({
            "type": "object",
            "properties": {
                "channel": {
                    "type": "string",
                    "maxLength": crate::schema::MAX_TEXT_FIELD_BYTES,
                    "description": "kanal, domyslnie default (do 64 KB)"
                },
                "message": {
                    "type": "string",
                    "maxLength": crate::schema::MAX_TEXT_FIELD_BYTES,
                    "description": "tresc powiadomienia (do 64 KB)"
                }
            },
            "required": ["message"],
            "additionalProperties": false
        })),
        "skill.load" => Some(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "nazwa skilla z listy w promptcie (bez sciezki)" }
            },
            "required": ["name"],
            "additionalProperties": false
        })),
        _ => None,
    }
}

pub fn list_tools() -> Vec<Value> {
    describe()
        .iter()
        .filter_map(|&(name, example, description)| {
            input_schema(name).map(|schema| {
                json!({
                    "name": name,
                    "description": description,
                    "inputSchema": schema,
                    "example": example
                })
            })
        })
        .collect()
}

/// Koperta kontraktu: każde wywołanie niesie identyfikatory zadania, sesji i kroku.
#[derive(Clone, Debug, Default)]
pub struct CallCtx {
    pub task_id: String,
    pub session_id: String,
    pub step_id: u64,
}

pub struct ToolResult {
    pub ok: bool,
    pub code: Option<String>,
    pub output: Value,
    pub bytes: usize,
}

impl ToolResult {
    pub fn success(output: Value, bytes: usize) -> ToolResult {
        ToolResult {
            ok: true,
            code: None,
            output,
            bytes,
        }
    }

    pub fn error(code: &str, message: impl Into<String>) -> ToolResult {
        let message = message.into();
        ToolResult {
            ok: false,
            code: Some(code.to_string()),
            output: json!({ "error": code, "message": message }),
            bytes: 0,
        }
    }

    /// Jednolinijkowe podsumowanie do transkryptu i promptu.
    pub fn summary(&self) -> String {
        if self.ok {
            self.output.to_string()
        } else {
            let message = self
                .output
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            format!("{}: {}", self.code.as_deref().unwrap_or("ERROR"), message)
        }
    }
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Ścieżka podana przez model, jeśli w argumencie jest.
pub fn path_arg(args: &Value) -> Option<String> {
    args.get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Zamienia ścieżkę względną na absolutną w workspace. Odrzuca `..`, ścieżki
/// bezwzględne i symlinki, które wyprowadzają poza workspace.
pub fn resolve(workspace: &Path, rel: &str) -> Result<PathBuf, ToolResult> {
    if rel.trim().is_empty() {
        return Err(ToolResult::error("SCHEMA_ERROR", "pusta sciezka"));
    }
    let candidate = Path::new(rel);
    let mut normals = 0usize;
    for part in candidate.components() {
        match part {
            Component::Normal(name) => {
                normals += 1;
                if name == ".agent" {
                    return Err(ToolResult::error(
                        "DENIED",
                        "katalog .agent jest zarezerwowany dla rdzenia",
                    ));
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(ToolResult::error(
                    "DENIED",
                    "sciezka zawiera '..'",
                ))
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(ToolResult::error(
                    "DENIED",
                    "sciezka bezwzgledna jest zabroniona",
                ))
            }
        }
    }
    // W1: "." albo sciezka degenerujaca sie do katalogu workspace to nie plik.
    if normals == 0 {
        return Err(ToolResult::error(
            "DENIED",
            "sciezka musi wskazywac plik w workspace, nie sam katalog",
        ));
    }

    let root = fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    let mut walk = root.clone();
    for part in candidate.components() {
        if let Component::Normal(name) = part {
            walk.push(name);
            match fs::symlink_metadata(&walk) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    let target = match fs::canonicalize(&walk) {
                        Ok(t) => t,
                        Err(e) => {
                            return Err(ToolResult::error(
                                "DENIED",
                                format!("nie mozna rozwiazac symlinku {}: {}", walk.display(), e),
                            ))
                        }
                    };
                    if !target.starts_with(&root) {
                        return Err(ToolResult::error(
                            "DENIED",
                            format!(
                                "sciezka przechodzi przez symlink poza workspace: {}",
                                walk.display()
                            ),
                        ));
                    }
                    walk = target;
                }
                Ok(_) => {}
                Err(_) => break, // jeszcze nie istnieje — dalszych czesci nie ma po co sprawdzac
            }
        }
    }
    let full = root.join(candidate);
    // F1: zakaz `.agent` musi dotyczyc celu po rozwiazaniu symlinkow, nie nazwy
    // komponentu. Inaczej alias -> `.agent` omija bramke.
    ensure_not_reserved(&root, &full)?;
    Ok(full)
}

/// Kanonizuje tyle sciezki, ile istnieje, i dokleja reszte bez zmiany.
/// Dzieki temu bramka `.agent` dziala takze dla plikow, ktorych jeszcze nie ma.
fn canonical_lenient(path: &Path) -> PathBuf {
    let mut prefix = path.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(actual) = fs::canonicalize(&prefix) {
            let mut out = actual;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (prefix.file_name(), prefix.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                prefix = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Porownuje scanonicalizowany cel z scanonicalizowanym katalogiem `.agent`.
fn ensure_not_reserved(root: &Path, full: &Path) -> Result<(), ToolResult> {
    let reserved = canonical_lenient(&root.join(".agent"));
    let target = canonical_lenient(full);
    if target.starts_with(&reserved) {
        return Err(ToolResult::error(
            "DENIED",
            "katalog .agent jest zarezerwowany dla rdzenia",
        ));
    }
    Ok(())
}

/// Faza B pkt 7: ostateczna weryfikacja tuz przed otwarciem pliku.
/// Kanonizuje to, co faktycznie istnieje (plik albo najblizszy istniejacy
/// przodek), i jeszcze raz sprawdza `starts_with(root)`. Chroni przed
/// podmiana sciezki miedzy `resolve` a otwarciem (TOCTOU).
pub fn resolve_open(workspace: &Path, rel: &str) -> Result<PathBuf, ToolResult> {
    let full = resolve(workspace, rel)?;
    let root = fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    match fs::canonicalize(&full) {
        Ok(actual) => {
            if !actual.starts_with(&root) {
                return Err(ToolResult::error(
                    "DENIED",
                    format!("sciezka prowadzi poza workspace: {}", rel),
                ));
            }
            Ok(actual)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let anchor_path = full
                .ancestors()
                .find(|a| a.exists())
                .map(|a| a.to_path_buf())
                .unwrap_or_else(|| workspace.to_path_buf());
            let anchor = fs::canonicalize(&anchor_path).map_err(|e| {
                ToolResult::error("IO_ERROR", format!("nie mozna rozwiazac {}: {}", rel, e))
            })?;
            if !anchor.starts_with(&root) {
                return Err(ToolResult::error(
                    "DENIED",
                    format!("sciezka prowadzi poza workspace: {}", rel),
                ));
            }
            Ok(full)
        }
        Err(e) => Err(ToolResult::error(
            "IO_ERROR",
            format!("nie mozna rozwiazac {}: {}", rel, e),
        )),
    }
}

/// Rosnacy licznik + czas + pid: nazwa pliku tymczasowego nie jest przewidywalna.
fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{}-{}-{}", std::process::id(), nanos, n)
}

fn err(code: &str, message: impl Into<String>) -> (String, String) {
    (code.to_string(), message.into())
}

/// Zapis atomowy. K2: plik tymczasowy powstaje przez `create_new` (O_EXCL) i
/// O_NOFOLLOW, wiec nie da sie podstawic symlinku pod jego nazwe. K3: katalog
/// docelowy jest kanonizowany i sprawdzany wzgledem korzenia przed zapisem
/// oraz po zmianie nazwy.
fn atomic_write(root: &Path, full: &Path, content: &str) -> Result<(), (String, String)> {
    let canon_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let parent = full.parent().unwrap_or(root);
    let canon_parent = fs::canonicalize(parent)
        .map_err(|e| err("IO_ERROR", format!("nie mozna rozwiazac katalogu: {}", e)))?;
    if !canon_parent.starts_with(&canon_root) {
        return Err(err(
            "DENIED",
            format!("katalog docelowy jest poza workspace: {}", canon_parent.display()),
        ));
    }
    let name = full
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plik");
    let tmp = canon_parent.join(format!(".{}.{}.tmp", name, unique_suffix()));

    let attempt = (|| -> std::io::Result<()> {
        use std::io::Write;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            // O_NOFOLLOW: jesli pod nazwa tymczasowa jest symlink, open konczy sie bledem.
            opts.custom_flags(0x20000);
        }
        let mut file = opts.open(&tmp)?;
        if !file.metadata()?.file_type().is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "cel zapisu nie jest zwyklym plikiem",
            ));
        }
        file.write_all(content.as_bytes())?;
        let _ = file.sync_all();
        fs::rename(&tmp, full)
    })();
    if attempt.is_err() {
        let _ = fs::remove_file(&tmp);
        return attempt.map_err(|e| err("IO_ERROR", format!("zapis nieudany: {}", e)));
    }
    match fs::canonicalize(full) {
        Ok(actual) if actual.starts_with(&canon_root) => Ok(()),
        Ok(actual) => {
            let _ = fs::remove_file(full);
            Err(err(
                "DENIED",
                format!("zapis wyszedl poza workspace: {}", actual.display()),
            ))
        }
        Err(e) => Err(err("IO_ERROR", format!("nie mozna potwierdzic zapisu: {}", e))),
    }
}

/// Wspolne wejscie `fs.write` i `fs.append`: `content` z limitami kontraktu.
fn content_arg<'a>(args: &'a Value) -> Result<&'a str, ToolResult> {
    let content = match args.get("content").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return Err(ToolResult::error("SCHEMA_ERROR", "brak argumentu 'content'")),
    };
    if content.len() > MAX_WRITE_BYTES {
        return Err(ToolResult::error(
            "DENIED",
            format!(
                "tresc ma {} bajtow, limit zapisu to {}",
                content.len(),
                MAX_WRITE_BYTES
            ),
        ));
    }
    // F25: `content` jest polem tekstowym, wiec obowiazuje go wezszy limit
    // 64 KB. Kolejnosc jest celowa: ponad 1 MB dalej daje DENIED (kontrakt
    // mcp.md i demo), a przedzial 64 KB..1 MB lapie ten walidator.
    if content.len() > crate::schema::MAX_TEXT_FIELD_BYTES {
        return Err(ToolResult::error(
            "SCHEMA_ERROR",
            format!(
                "argument 'content' ma {} bajtow, limit pola tekstowego to {}",
                content.len(),
                crate::schema::MAX_TEXT_FIELD_BYTES
            ),
        ));
    }
    Ok(content)
}

pub fn call(name: &str, args: &Value, workspace: &Path) -> ToolResult {
    let schema = match input_schema(name) {
        Some(s) => s,
        None => return ToolResult::error("DENIED", format!("nieznane narzedzie: {}", name)),
    };
    // Faza B, pkt 5: argumenty sprawdzone względem schematu z tools/list, zanim cokolwiek
    // dotknie dysku. Ten sam walidator obsługuje MCP i pętlę agenta.
    if let Err(why) = crate::schema::validate(&schema, args) {
        return ToolResult::error("SCHEMA_ERROR", why);
    }

    match name {
        "fs.read" => {
            let rel = match path_arg(args) {
                Some(p) => p,
                None => return ToolResult::error("SCHEMA_ERROR", "brak argumentu 'path'"),
            };
            let full = match resolve_open(workspace, &rel) {
                Ok(p) => p,
                Err(e) => return e,
            };
            let meta = match fs::metadata(&full) {
                Ok(m) => m,
                Err(e) => {
                    let code = if e.kind() == std::io::ErrorKind::NotFound {
                        "NOT_FOUND"
                    } else {
                        "IO_ERROR"
                    };
                    return ToolResult::error(code, format!("nie mozna odczytac {}: {}", rel, e));
                }
            };
            if meta.len() as usize > MAX_READ_BYTES {
                return ToolResult::error(
                    "IO_ERROR",
                    format!(
                        "plik {} ma {} bajtow, limit odczytu to {}",
                        rel,
                        meta.len(),
                        MAX_READ_BYTES
                    ),
                );
            }
            match fs::read(&full) {
                Ok(bytes) => {
                    let size = bytes.len();
                    let content = String::from_utf8_lossy(&bytes).to_string();
                    ToolResult::success(json!({ "content": content, "size": size }), size)
                }
                Err(e) => ToolResult::error("IO_ERROR", format!("nie mozna odczytac {}: {}", rel, e)),
            }
        }
        "fs.write" => {
            let rel = match path_arg(args) {
                Some(p) => p,
                None => return ToolResult::error("SCHEMA_ERROR", "brak argumentu 'path'"),
            };
            let content = match content_arg(args) {
                Ok(c) => c,
                Err(e) => return e,
            };
            let full = match resolve_open(workspace, &rel) {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Some(parent) = full.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return ToolResult::error(
                        "IO_ERROR",
                        format!("nie mozna utworzyc katalogu: {}", e),
                    );
                }
            }
            match atomic_write(workspace, &full, content) {
                Ok(()) => ToolResult::success(json!({ "bytes_written": content.len() }), content.len()),
                Err((code, msg)) => ToolResult::error(
                    &code,
                    format!("nie mozna zapisac {}: {}", rel, msg),
                ),
            }
        }
        "fs.append" => {
            let rel = match path_arg(args) {
                Some(p) => p,
                None => return ToolResult::error("SCHEMA_ERROR", "brak argumentu 'path'"),
            };
            let content = match content_arg(args) {
                Ok(c) => c,
                Err(e) => return e,
            };
            let full = match resolve_open(workspace, &rel) {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Some(parent) = full.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return ToolResult::error(
                        "IO_ERROR",
                        format!("nie mozna utworzyc katalogu: {}", e),
                    );
                }
            }
            let canon_root =
                fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
            let parent = full.parent().unwrap_or(workspace);
            let canon_parent = match fs::canonicalize(parent) {
                Ok(p) => p,
                Err(e) => {
                    return ToolResult::error(
                        "IO_ERROR",
                        format!("nie mozna rozwiazac katalogu: {}", e),
                    )
                }
            };
            if !canon_parent.starts_with(&canon_root) {
                return ToolResult::error(
                    "DENIED",
                    format!(
                        "katalog docelowy jest poza workspace: {}",
                        canon_parent.display()
                    ),
                );
            }
            let name = full.file_name().and_then(|n| n.to_str()).unwrap_or("plik");
            let target = canon_parent.join(name);
            // Istniejacy cel musi byc zwyklym plikiem; symlink konczacy sciezke
            // jest odrzucany tak samo jak przy odczycie.
            match fs::symlink_metadata(&target) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return ToolResult::error(
                        "DENIED",
                        format!("cel dopisu jest symlinkiem: {}", rel),
                    )
                }
                Ok(meta) if !meta.file_type().is_file() => {
                    return ToolResult::error(
                        "DENIED",
                        format!("cel dopisu nie jest zwyklym plikiem: {}", rel),
                    )
                }
                Ok(_) => {
                    let actual = match fs::canonicalize(&target) {
                        Ok(p) => p,
                        Err(e) => {
                            return ToolResult::error(
                                "IO_ERROR",
                                format!("nie mozna rozwiazac {}: {}", rel, e),
                            )
                        }
                    };
                    if !actual.starts_with(&canon_root) {
                        return ToolResult::error(
                            "DENIED",
                            format!("sciezka prowadzi poza workspace: {}", rel),
                        );
                    }
                }
                Err(_) => {}
            }
            let attempt = (|| -> std::io::Result<usize> {
                use std::io::Write;
                let mut opts = fs::OpenOptions::new();
                opts.append(true).create(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    // O_NOFOLLOW: ostatni komponent nie da sie podmienic na symlink.
                    opts.custom_flags(0x20000);
                }
                let mut file = opts.open(&target)?;
                if !file.metadata()?.file_type().is_file() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "cel dopisu nie jest zwyklym plikiem",
                    ));
                }
                file.write_all(content.as_bytes())?;
                let _ = file.sync_all();
                Ok(content.len())
            })();
            match attempt {
                Ok(written) => ToolResult::success(json!({ "bytes_written": written }), written),
                Err(e) => ToolResult::error(
                    "IO_ERROR",
                    format!("nie mozna dopisac {}: {}", rel, e),
                ),
            }
        }
        // Runda 16: `skill.load` — te same bramki co inne narzędzia: allowlista
        // (sprawdzana w pętli), brak ścieżek (nazwa nie może być ścieżką),
        // nazwa musi być na odkrytej liście.
        "skill.load" => {
            let name = args
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .unwrap_or("");
            if name.is_empty() {
                return ToolResult::error("SCHEMA_ERROR", "brak argumentu 'name'");
            }
            if crate::skills::is_path_like(name) {
                return ToolResult::error(
                    "DENIED",
                    format!("nazwa skilla nie moze byc sciezka: {}", name),
                );
            }
            let skills = crate::skills::discover(workspace);
            let Some(skill) = crate::skills::find(&skills, name) else {
                return ToolResult::error(
                    "NOT_FOUND",
                    format!("skill '{}' nie jest na liscie", name),
                );
            };
            if let Some(why) = &skill.header_error {
                return ToolResult::error(
                    "SCHEMA_ERROR",
                    format!("skill '{}' ma blad naglowka: {}", name, why),
                );
            }
            let (content, truncated) = skill.clipped();
            let bytes = content.len();
            let mut output = json!({
                "content": content,
                "bytes": bytes,
                "truncated": truncated,
            });
            if truncated {
                output["warning"] = json!(format!(
                    "tresc obcieta do {} bajtow (skill ma {})",
                    crate::skills::MAX_CONTENT_BYTES,
                    skill.size()
                ));
            }
            ToolResult::success(output, bytes)
        }
        "notify.send" => {
            let msg = match args.get("message").and_then(|v| v.as_str()) {
                Some(m) if !m.trim().is_empty() => m,
                _ => return ToolResult::error("SCHEMA_ERROR", "brak argumentu 'message'"),
            };
            // F25: drugie zabezpieczenie tego samego limitu, zeby zmiana schematu
            // nie otworzyla z powrotem drogi do zapisu megabajtow w logu.
            if msg.len() > crate::schema::MAX_TEXT_FIELD_BYTES {
                return ToolResult::error(
                    "SCHEMA_ERROR",
                    format!(
                        "message ma {} bajtow, limit to {}",
                        msg.len(),
                        crate::schema::MAX_TEXT_FIELD_BYTES
                    ),
                );
            }
            let channel = args
                .get("channel")
                .and_then(|v| v.as_str())
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or("default");
            let dir = workspace.join(".agent").join("outbox");
            if let Err(e) = fs::create_dir_all(&dir) {
                return ToolResult::error("IO_ERROR", format!("nie mozna utworzyc outbox: {}", e));
            }
            let file = dir.join("notifications.log");
            let line = format!("[{}] channel={} {}\n", now_ts(), channel, msg.trim());
            match fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file)
            {
                Ok(mut f) => {
                    use std::io::Write;
                    match f.write_all(line.as_bytes()) {
                        Ok(()) => ToolResult::success(
                            json!({ "status": "queued", "channel": channel }),
                            line.len(),
                        ),
                        Err(e) => ToolResult::error(
                            "IO_ERROR",
                            format!("nie mozna zapisac powiadomienia: {}", e),
                        ),
                    }
                }
                Err(e) => ToolResult::error("IO_ERROR", format!("nie mozna otworzyc outbox: {}", e)),
            }
        }
        other => ToolResult::error("DENIED", format!("nieznane narzedzie: {}", other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn temp_ws(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-tools-{}-{}-{}",
            tag,
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn seed_secret(ws: &Path) -> PathBuf {
        let agent = ws.join(".agent");
        fs::create_dir_all(&agent).unwrap();
        let secret = agent.join("secret.txt");
        fs::write(&secret, "sekret").unwrap();
        secret
    }

    #[test]
    fn read_through_symlink_to_agent_is_denied() {
        let ws = temp_ws("alias-read");
        let secret = seed_secret(&ws);
        symlink(".agent", ws.join("alias")).unwrap();

        let direct = call("fs.read", &json!({ "path": ".agent/secret.txt" }), &ws);
        assert_eq!(
            direct.code.as_deref(),
            Some("DENIED"),
            "kontrola: bezposrednia sciezka do .agent"
        );

        let via_alias = call("fs.read", &json!({ "path": "alias/secret.txt" }), &ws);
        assert!(
            !via_alias.ok,
            "odczyt przez alias do .agent musi byc odrzucony, bylo ok={}",
            via_alias.ok
        );
        assert_eq!(via_alias.code.as_deref(), Some("DENIED"));

        assert_eq!(fs::read_to_string(&secret).unwrap(), "sekret");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn write_through_symlink_to_agent_is_denied() {
        let ws = temp_ws("alias-write");
        let secret = seed_secret(&ws);
        symlink(".agent", ws.join("alias")).unwrap();

        let r = call(
            "fs.write",
            &json!({ "path": "alias/secret.txt", "content": "podmiana" }),
            &ws,
        );
        assert!(!r.ok, "zapis przez alias do .agent musi byc odrzucony");
        assert_eq!(r.code.as_deref(), Some("DENIED"));
        assert_eq!(
            fs::read_to_string(&secret).unwrap(),
            "sekret",
            "plik w .agent nie moze zostac nadpisany"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn nested_symlink_into_agent_is_denied() {
        let ws = temp_ws("nested");
        let secret = seed_secret(&ws);
        fs::create_dir_all(ws.join("dane")).unwrap();
        symlink("../.agent", ws.join("dane/link")).unwrap();

        let r = call("fs.read", &json!({ "path": "dane/link/secret.txt" }), &ws);
        assert_eq!(r.code.as_deref(), Some("DENIED"), "zagniezdzony symlink do .agent");
        assert_eq!(fs::read_to_string(&secret).unwrap(), "sekret");
        let _ = fs::remove_dir_all(&ws);
    }

    /// F25: pole tekstowe ma miec limit 64 KB (specyfikacja zadania, nie stala
    /// z implementacji). 3 MB `message` musi byc odrzucone kodem SCHEMA_ERROR,
    /// zanim cokolwiek dotknie dysku.
    const TEXT_LIMIT: usize = 64 * 1024;
    const THREE_MB: usize = 3 * 1024 * 1024;

    #[test]
    fn oversized_message_is_schema_error_and_writes_nothing() {
        let ws = temp_ws("notify-big");
        let msg = "a".repeat(THREE_MB);
        let r = call("notify.send", &json!({ "message": msg }), &ws);
        assert_eq!(
            r.code.as_deref(),
            Some("SCHEMA_ERROR"),
            "3 MB message musi byc odrzucone przez walidacje, ok={} output={}",
            r.ok,
            r.output
        );
        let outbox = ws.join(".agent").join("outbox");
        assert!(
            !outbox.join("notifications.log").exists(),
            "odrzucony komunikat nie moze trafic do logu"
        );
        assert!(
            !outbox.exists(),
            "dla odrzuconego wywolania nie powstaje nawet katalog outbox"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    /// Granica: komunikat rowny limitowi jest legalny, o jeden bajt wiekszy nie.
    #[test]
    fn message_at_the_text_limit_is_accepted() {
        let ws = temp_ws("notify-limit");
        let ok = call(
            "notify.send",
            &json!({ "message": "a".repeat(TEXT_LIMIT) }),
            &ws,
        );
        assert!(ok.ok, "komunikat rowny limitowi ma przejsc: {:?}", ok.code);

        let too_big = call(
            "notify.send",
            &json!({ "message": "a".repeat(TEXT_LIMIT + 1) }),
            &ws,
        );
        assert_eq!(too_big.code.as_deref(), Some("SCHEMA_ERROR"));
        let _ = fs::remove_dir_all(&ws);
    }

    /// Ten sam limit dla `channel`: nazwa kanalu tez trafia do linii logu.
    #[test]
    fn oversized_channel_is_schema_error() {
        let ws = temp_ws("notify-channel");
        let r = call(
            "notify.send",
            &json!({ "channel": "c".repeat(TEXT_LIMIT + 1), "message": "ok" }),
            &ws,
        );
        assert_eq!(r.code.as_deref(), Some("SCHEMA_ERROR"), "za dlugi channel");
        assert!(!ws.join(".agent").exists());
        let _ = fs::remove_dir_all(&ws);
    }

    /// F25 dla `content`: pole tekstowe fs.write ma ten sam limit 64 KB.
    /// Powyzej 1 MB zostaje stary kod kontraktu (DENIED), bo tak mowi mcp.md
    /// i tak sprawdza to demo.
    #[test]
    fn oversized_write_content_is_rejected_and_nothing_is_written() {
        let ws = temp_ws("write-big");
        let just_over = call(
            "fs.write",
            &json!({ "path": "duzy.txt", "content": "a".repeat(TEXT_LIMIT + 1) }),
            &ws,
        );
        assert_eq!(
            just_over.code.as_deref(),
            Some("SCHEMA_ERROR"),
            "64 KB + 1 bajt tresci to juz za duze pole tekstowe"
        );
        assert!(!ws.join("duzy.txt").exists(), "nic nie trafia na dysk");

        let way_over = call(
            "fs.write",
            &json!({ "path": "duzy.txt", "content": "a".repeat(MAX_WRITE_BYTES + 1) }),
            &ws,
        );
        assert_eq!(
            way_over.code.as_deref(),
            Some("DENIED"),
            "ponad 1 MB to nadal limit kontraktu"
        );
        assert!(!ws.join("duzy.txt").exists());

        let at_limit = call(
            "fs.write",
            &json!({ "path": "ok.txt", "content": "a".repeat(TEXT_LIMIT) }),
            &ws,
        );
        assert!(at_limit.ok, "dokladnie 64 KB ma przejsc: {:?}", at_limit.code);
        let _ = fs::remove_dir_all(&ws);
    }

    /// Schemat wystawiany w `tools/list` musi deklarowac ten limit, zeby klient
    /// wiedzial o nim przed wyslaniem argumentow.
    #[test]
    fn notify_schema_declares_text_limit() {
        let schema = input_schema("notify.send").expect("schema notify.send");
        let max = schema["properties"]["message"]["maxLength"].as_u64();
        assert_eq!(max, Some(TEXT_LIMIT as u64), "maxLength dla message");
        assert_eq!(
            schema["properties"]["channel"]["maxLength"].as_u64(),
            Some(TEXT_LIMIT as u64),
            "maxLength dla channel"
        );
    }

    #[test]
    fn symlink_outside_workspace_is_still_denied() {
        let ws = temp_ws("outside");
        let outside = temp_ws("outside-target");
        fs::write(outside.join("tajne.txt"), "tajne").unwrap();
        symlink(&outside, ws.join("link")).unwrap();

        let r = call("fs.read", &json!({ "path": "link/tajne.txt" }), &ws);
        assert_eq!(r.code.as_deref(), Some("DENIED"));
        let _ = fs::remove_dir_all(&ws);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn append_creates_file_and_reports_bytes() {
        let ws = temp_ws("append-create");
        let r = call(
            "fs.append",
            &json!({ "path": "nowy.txt", "content": "abc" }),
            &ws,
        );
        assert!(r.ok, "{:?} {}", r.code, r.output);
        assert_eq!(r.output["bytes_written"], json!(3));
        assert_eq!(fs::read_to_string(ws.join("nowy.txt")).unwrap(), "abc");

        let more = call(
            "fs.append",
            &json!({ "path": "nowy.txt", "content": "de" }),
            &ws,
        );
        assert!(more.ok, "{:?} {}", more.code, more.output);
        assert_eq!(more.output["bytes_written"], json!(2));
        assert_eq!(fs::read_to_string(ws.join("nowy.txt")).unwrap(), "abcde");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn append_outside_workspace_is_denied_and_creates_nothing() {
        let ws = temp_ws("append-escape");
        let name = format!("poza-{}.txt", unique_suffix());
        let r = call(
            "fs.append",
            &json!({ "path": format!("../{}", name), "content": "x" }),
            &ws,
        );
        assert_eq!(r.code.as_deref(), Some("DENIED"));
        assert!(
            !ws.parent().unwrap().join(&name).exists(),
            "plik poza workspace nie moze powstac"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn append_through_alias_to_agent_is_denied() {
        let ws = temp_ws("append-alias");
        let secret = seed_secret(&ws);
        symlink(".agent", ws.join("alias")).unwrap();

        let r = call(
            "fs.append",
            &json!({ "path": "alias/secret.txt", "content": "dopisek" }),
            &ws,
        );
        assert_eq!(r.code.as_deref(), Some("DENIED"));
        assert_eq!(
            fs::read_to_string(&secret).unwrap(),
            "sekret",
            "plik w .agent nie moze zostac zmieniony"
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn append_is_exported_like_write() {
        assert!(NAMES.contains(&"fs.append"), "fs.append musi byc w NAMES");
        let schema = input_schema("fs.append").expect("schema fs.append");
        assert_eq!(schema["required"], json!(["path", "content"]));
        let listed: Vec<String> = list_tools()
            .iter()
            .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
            .collect();
        assert!(
            listed.contains(&"fs.append".to_string()),
            "tools/list musi wystawiac fs.append: {:?}",
            listed
        );
    }

    fn seed_skill(ws: &Path, dir: &str, header: &str, body: &str) {
        let target = ws.join("skills").join(dir);
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("SKILL.md"), format!("{}\n{}\n", header, body)).unwrap();
    }

    #[test]
    fn skill_load_returns_the_body_without_the_header() {
        let ws = temp_ws("skill-ok");
        seed_skill(
            &ws,
            "raport",
            "---\nname: raport\ndescription: dzienny raport\n---",
            "Zawsze podaj trzy liczby.",
        );
        let r = call("skill.load", &json!({ "name": "raport" }), &ws);
        assert!(r.ok, "{:?}", r.output);
        assert_eq!(r.code, None);
        assert_eq!(
            r.output.get("content").and_then(|v| v.as_str()),
            Some("Zawsze podaj trzy liczby.")
        );
        let bytes = r.output.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(bytes as usize, "Zawsze podaj trzy liczby.".len());
        assert_eq!(
            r.output.get("truncated").and_then(|v| v.as_bool()),
            Some(false)
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn skill_load_outside_the_list_is_not_found() {
        let ws = temp_ws("skill-404");
        let r = call("skill.load", &json!({ "name": "nie-ma" }), &ws);
        assert!(!r.ok);
        assert_eq!(r.code.as_deref(), Some("NOT_FOUND"), "{:?}", r.output);

        // Ścieżka to nie nazwa — DENIED, jak przy reszcie perimetru.
        let path = call("skill.load", &json!({ "name": "../etc/passwd" }), &ws);
        assert_eq!(path.code.as_deref(), Some("DENIED"), "{:?}", path.output);
        let empty = call("skill.load", &json!({}), &ws);
        assert_eq!(empty.code.as_deref(), Some("SCHEMA_ERROR"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn skill_content_over_8kb_is_clipped_with_a_warning() {
        let ws = temp_ws("skill-big");
        let body = "x".repeat(crate::skills::MAX_CONTENT_BYTES + 500);
        seed_skill(
            &ws,
            "duzy",
            "---\nname: duzy\ndescription: duzy skill\n---",
            &body,
        );
        let r = call("skill.load", &json!({ "name": "duzy" }), &ws);
        assert!(r.ok, "{:?}", r.output);
        let content = r.output.get("content").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(content.len(), crate::skills::MAX_CONTENT_BYTES);
        assert_eq!(
            r.output.get("truncated").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert!(
            r.output.get("warning").and_then(|v| v.as_str()).is_some(),
            "obciecie musi miec ostrzezenie: {:?}",
            r.output
        );
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn skill_load_is_read_only_and_never_drifts() {
        assert!(!is_mutating("skill.load"));
        assert!(NAMES.contains(&"skill.load"));
    }
}
