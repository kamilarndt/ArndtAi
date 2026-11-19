//! Dwa mózgi agenta: offline (deterministyczny, do testów) i zdalny
//! (Anthropic lub OpenAI-compatible). HTTP idzie przez `curl`, żeby nie
//! dociągać ciężkich zależności.

use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Turn {
    pub role: String,
    pub content: String,
}

#[derive(Debug)]
pub enum Action {
    Tool { name: String, args: Value },
    Finish { summary: String },
}

#[derive(Debug)]
pub struct Decision {
    pub thought: String,
    pub action: Action,
}

pub enum Brain {
    Mock(MockBrain),
    Remote(RemoteBrain),
}

#[derive(Clone, Copy)]
pub enum RemoteKind {
    Anthropic,
    OpenAi,
}

pub struct RemoteBrain {
    kind: RemoteKind,
    model: String,
    base_url: String,
    api_key: String,
    /// Identyfikator sesji dla bramki OC-Go (`x-opencode-session`).
    session: String,
    /// F9: prompt reklamuje tylko narzedzia przepuszczone przez allowliste.
    allowed: Vec<String>,
    /// Runda 16: lista skilli (`- nazwa: opis`) w promptcie systemowym.
    skills: Vec<String>,
    /// Runda 12: zdania o probach naprawy, ktore agent dopisuje do transkryptu
    /// zaraz po `next` jako zdarzenie `repair`.
    repairs: Vec<String>,
    /// Runda 14: sufit tokenow odpowiedzi z `limit.output` (nie wiecej niz
    /// [`MAX_TOKENS`], dopoki nie ma osobnego ustawienia).
    max_tokens: usize,
}

/// Adres i klucz zdalnego providera — tyle, ile potrzebuje `GET /models`
/// i picker modeli. Klucz nie opuszcza procesu poza plik `--config` curl.
#[derive(Clone)]
pub struct RemoteConn {
    pub kind: RemoteKind,
    pub base_url: String,
    pub api_key: String,
    pub session: String,
}

/// `usage` z ostatniej odpowiedzi modelu. Jest tylko wtedy, gdy provider
/// naprawdę ją przysłał — inaczej TUI liczy z znaków i oznacza to `~`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub prompt: u64,
    pub completion: u64,
}


pub struct MockBrain {
    script: Vec<Action>,
    pos: usize,
    note: String,
}

impl Brain {
    pub fn from_env(provider: Option<&str>, task: &str, allowed: &[String]) -> Brain {
        Brain::from_env_full(provider, task, allowed, None, None)
    }

    /// Runda 14: `model` to wybór z `/model` (albo z `.agent/config`);
    /// `max_tokens` to `limit.output` wybranego modelu, przycięty do
    /// [`MAX_TOKENS`]. `ARNDTAI_MODEL` nadal wygrywa z oboma.
    pub fn from_env_full(
        provider: Option<&str>,
        task: &str,
        allowed: &[String],
        model: Option<&str>,
        max_tokens: Option<usize>,
    ) -> Brain {
        let explicit = provider.map(|p| p.trim().to_lowercase());

        match explicit.as_deref() {
            Some("mock") | Some("offline") | Some("test") => {
                return Brain::Mock(MockBrain::new(task, String::new()))
            }
            _ => {}
        }

        let anthropic_key = env::var("ANTHROPIC_API_KEY").ok().filter(|s| !s.is_empty());
        let openai_key = env::var("OPENAI_API_KEY").ok().filter(|s| !s.is_empty());
        let generic_key = env::var("ARNDTAI_API_KEY").ok().filter(|s| !s.is_empty());

        let kind = match explicit.as_deref() {
            Some("anthropic") => Some(RemoteKind::Anthropic),
            Some("openai") | Some("http") => Some(RemoteKind::OpenAi),
            Some(other) => {
                return Brain::Mock(MockBrain::new(
                    task,
                    format!("nieznany provider '{}', uzywam trybu offline", other),
                ))
            }
            None => {
                if anthropic_key.is_some() || generic_key.is_some() {
                    Some(RemoteKind::Anthropic)
                } else if openai_key.is_some() {
                    Some(RemoteKind::OpenAi)
                } else {
                    None
                }
            }
        };

        let kind = match kind {
            Some(k) => k,
            None => {
                return Brain::Mock(MockBrain::new(
                    task,
                    "brak klucza API, uzywam trybu offline".to_string(),
                ))
            }
        };

        let (key, default_model, default_base, label) = match kind {
            RemoteKind::Anthropic => (
                anthropic_key.or(generic_key),
                "claude-sonnet-5-5".to_string(),
                "https://api.anthropic.com".to_string(),
                "anthropic",
            ),
            RemoteKind::OpenAi => (
                openai_key.or(generic_key),
                "gpt-4o".to_string(),
                "https://api.openai.com/v1".to_string(),
                "openai",
            ),
        };

        let api_key = match key {
            Some(k) => k,
            None => {
                return Brain::Mock(MockBrain::new(
                    task,
                    format!("wybrano '{}', ale brak klucza API", label),
                ))
            }
        };

        let model = env::var("ARNDTAI_MODEL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| model.map(|m| m.trim().to_string()).filter(|m| !m.is_empty()))
            .unwrap_or(default_model);
        let base_url = env::var("ARNDTAI_BASE_URL").unwrap_or(default_base);

        Brain::Remote(RemoteBrain {
            kind,
            model,
            base_url,
            api_key,
            session: session_id(),
            allowed: allowed.to_vec(),
            skills: Vec::new(),
            repairs: Vec::new(),
            max_tokens: max_tokens.unwrap_or(MAX_TOKENS).min(MAX_TOKENS),
        })
    }

    /// Runda 16: lista skilli do promptu systemowego (maks. 20 pozycji,
    /// formatowanie w `skills::prompt_lines`). Mock nic nie wysyla, więc
    /// wywołanie jest tam pustym krokiem.
    pub fn set_skills(&mut self, skills: Vec<String>) {
        if let Brain::Remote(r) = self {
            r.skills = skills;
        }
    }

    pub fn next(
        &mut self,
        task: &str,
        prefix: &str,
        turns: &[Turn],
    ) -> Result<Decision, String> {
        match self {
            Brain::Mock(m) => Ok(m.next()),
            Brain::Remote(r) => r.next(task, prefix, turns),
        }
    }

    /// Prob naprawy, ktore zdarzyly sie od ostatniego wywolania. Agent dopisuje
    /// je do transkryptu jako `repair`; kazda trafia tez do statusu przebiegu.
    pub fn take_repairs(&mut self) -> Vec<String> {
        match self {
            Brain::Mock(_) => Vec::new(),
            Brain::Remote(r) => r.take_repairs(),
        }
    }

    pub fn label(&self) -> String {
        match self {
            Brain::Mock(_) => "offline (mock)".to_string(),
            Brain::Remote(r) => {
                let kind = match r.kind {
                    RemoteKind::Anthropic => "anthropic",
                    RemoteKind::OpenAi => "openai",
                };
                format!("{} / {}", kind, r.model)
            }
        }
    }

    /// Sam model, bez dostawcy — tak podpisuje composer w Grok Build
    /// (`Grok 4.7 (medium)`). Mock nie ma modelu, więc mówi wprost, czym jest.
    pub fn model_name(&self) -> String {
        match self {
            Brain::Mock(_) => "offline (mock)".to_string(),
            Brain::Remote(r) => r.model.clone(),
        }
    }

    /// Adres i klucz zdalnego providera do `GET /models`; `None` w trybie
    /// offline (wtedy nie ma czego pytać, a licznik pokaże `?`).
    pub fn conn(&self) -> Option<RemoteConn> {
        match self {
            Brain::Mock(_) => None,
            Brain::Remote(r) => Some(RemoteConn {
                kind: r.kind,
                base_url: r.base_url.clone(),
                api_key: r.api_key.clone(),
                session: r.session.clone(),
            }),
        }
    }

    pub fn note(&self) -> Option<&str> {
        match self {
            Brain::Mock(m) => {
                if m.note.is_empty() {
                    None
                } else {
                    Some(m.note.as_str())
                }
            }
            Brain::Remote(_) => None,
        }
    }
}

impl MockBrain {
    pub fn new(task: &str, note: String) -> MockBrain {
        let mut script = Vec::new();
        for raw in task.split(|c| c == '\n' || c == ';') {
            let piece = raw.trim();
            if piece.is_empty() {
                continue;
            }
            let (verb, rest) = match piece.split_once(char::is_whitespace) {
                Some((v, r)) => (v.to_lowercase(), r.trim()),
                None => (piece.to_lowercase(), ""),
            };
            match verb.as_str() {
                "read" | "czytaj" => {
                    if !rest.is_empty() {
                        script.push(Action::Tool {
                            name: "fs.read".to_string(),
                            args: json!({ "path": rest }),
                        });
                    }
                }
                "write" | "zapisz" => {
                    if let Some((path, content)) = rest.split_once("::") {
                        script.push(Action::Tool {
                            name: "fs.write".to_string(),
                            args: json!({ "path": path.trim(), "content": content.trim() }),
                        });
                    }
                }
                "append" | "dopisz" => {
                    if let Some((path, content)) = rest.split_once("::") {
                        script.push(Action::Tool {
                            name: "fs.append".to_string(),
                            args: json!({ "path": path.trim(), "content": content.trim() }),
                        });
                    }
                }
                "notify" | "powiadom" => {
                    if !rest.is_empty() {
                        script.push(Action::Tool {
                            name: "notify.send".to_string(),
                            args: json!({ "message": rest }),
                        });
                    }
                }
                _ => {}
            }
        }
        MockBrain {
            script,
            pos: 0,
            note,
        }
    }

    fn next(&mut self) -> Decision {
        if self.pos < self.script.len() {
            let idx = self.pos;
            self.pos += 1;
            let label = match &self.script[idx] {
                Action::Tool { name, .. } => name.clone(),
                Action::Finish { .. } => "finish".to_string(),
            };
            return Decision {
                thought: format!("krok {} z planu offline: {}", idx + 1, label),
                action: take_action(&mut self.script, idx),
            };
        }
        Decision {
            thought: "plan offline wykonany".to_string(),
            action: Action::Finish {
                summary: "Zadanie wykonane w trybie offline.".to_string(),
            },
        }
    }
}

fn take_action(script: &mut Vec<Action>, idx: usize) -> Action {
    // Wyciągamy akcję bez kopiowania enumów z wnętrza wektora.
    let placeholder = Action::Finish {
        summary: String::new(),
    };
    let action = std::mem::replace(&mut script[idx], placeholder);
    action
}

// ── Sesja bramki OC-Go ──────────────────────────────────────────────────────
//
// Endpoint `https://opencode.ai/inference/go/openai/v1` (OpenAI-compatible)
// odrzuca żądanie bez nagłówka `x-opencode-session` (odpowiedź: `MissingSessionID`).
// Identyfikator bierzemy z `ARNDTAI_SESSION`, a gdy go brak — losujemy raz na
// proces. Nagłówek idzie tam, gdzie reszta: do pliku `--config` curl (0600),
// nigdy do argv.

fn generated_session() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{:x}{:x}{:x}", std::process::id(), nanos, seq)
}

/// `ARNDTAI_SESSION`, jeśli podany i niepusty; inaczej świeży identyfikator.
pub fn resolve_session(explicit: Option<&str>) -> String {
    explicit
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(generated_session)
}

/// Identyfikator sesji tego procesu — ten sam dla każdego żądania i dla TUI.
pub fn session_id() -> String {
    static SESSION: OnceLock<String> = OnceLock::new();
    SESSION
        .get_or_init(|| resolve_session(env::var("ARNDTAI_SESSION").ok().as_deref()))
        .clone()
}

/// Gorny limit tokenow odpowiedzi. 1200 ucinalo JSON przy dluzszym `thought`
/// albo `args`: model urywal obiekt i zadanie konczylo sie `FAILED_ERROR`
/// bez zadnej proby naprawy (runda 12).
pub const MAX_TOKENS: usize = 4096;

// ── `usage` z odpowiedzi (runda 14) ────────────────────────────────────────
//
// Licznik kontekstu bierze realne tokeny tylko wtedy, gdy provider je przysłał:
// OpenAI-compatible w `usage.prompt_tokens` / `usage.completion_tokens`,
// Anthropic w `usage.input_tokens` / `usage.output_tokens`. Ostatnia odpowiedź
// jest źródłem prawdy — prompt już zawiera całą historię, więc sumowanie
// promptów z kolejnych kroków liczyłoby historię wielokrotnie.

static LAST_PROMPT: AtomicU64 = AtomicU64::new(0);
static LAST_COMPLETION: AtomicU64 = AtomicU64::new(0);

/// Zapisuje `usage` z odpowiedzi; brak pola nie zeruje poprzedniego odczytu.
pub fn note_usage(response: &Value) {
    let Some(usage) = response.get("usage") else {
        return;
    };
    let prompt = usage
        .get("prompt_tokens")
        .or_else(|| usage.get("input_tokens"))
        .and_then(|v| v.as_u64());
    let completion = usage
        .get("completion_tokens")
        .or_else(|| usage.get("output_tokens"))
        .and_then(|v| v.as_u64());
    if let Some(n) = prompt {
        LAST_PROMPT.store(n, Ordering::Relaxed);
    }
    if let Some(n) = completion {
        LAST_COMPLETION.store(n, Ordering::Relaxed);
    }
}

/// Ostatnie znane `usage`; `None`, gdy provider nigdy go nie przysłał.
pub fn last_usage() -> Option<Usage> {
    let prompt = LAST_PROMPT.load(Ordering::Relaxed);
    let completion = LAST_COMPLETION.load(Ordering::Relaxed);
    if prompt == 0 && completion == 0 {
        None
    } else {
        Some(Usage { prompt, completion })
    }
}

/// Nowa sesja/tura: licznik nie może dziedziczyć `usage` poprzedniej.
pub fn reset_usage() {
    LAST_PROMPT.store(0, Ordering::Relaxed);
    LAST_COMPLETION.store(0, Ordering::Relaxed);
}

/// Ile ostatnich znakow surowej odpowiedzi trafia do komunikatu bledu.
pub const TAIL_CHARS: usize = 160;

/// Instrukcja doklejana wylacznie do zadania naprawczego (druga proba).
pub const REPAIR_INSTRUCTION: &str = "\n\nUWAGA: poprzednia odpowiedz nie byla poprawnym JSON-em. \
     Zwroc WYLACZNIE poprawny JSON, bez prozy i bez markdown: jeden obiekt \
     {\"thought\":\"...\",\"tool\":\"fs.read|fs.write|fs.append|notify.send\",\"args\":{...}} \
     albo {\"thought\":\"...\",\"finish\":\"podsumowanie\"}.";

/// Jedno zadanie do modelu: adres, cialo i naglowki (klucz nigdy w argv).
struct Wire {
    url: String,
    body: String,
    headers: Vec<String>,
}

impl RemoteBrain {
    fn next(&mut self, task: &str, prefix: &str, turns: &[Turn]) -> Result<Decision, String> {
        // Transport zdalny: `curl` + wyciagniecie tekstu z odpowiedzi dostawcy.
        // Samo doklejenie proby naprawy siedzi w `converse`, wiec testy liczą
        // zadania bez sieci.
        let kind = self.kind;
        self.converse(task, prefix, turns, move |wire: &Wire| {
            let response = http_post(&wire.url, &wire.body, &wire.headers)?;
            note_usage(&response);
            let text = match kind {
                RemoteKind::Anthropic => response
                    .pointer("/content/0/text")
                    .and_then(|v| v.as_str()),
                RemoteKind::OpenAi => response
                    .pointer("/choices/0/message/content")
                    .and_then(|v| v.as_str()),
            }
            .map(|s| s.to_string());

            text.ok_or_else(|| {
                let preview = truncate(&response.to_string(), 400);
                format!("odpowiedz modelu bez tresci: {}", preview)
            })
        })
    }

    /// Jedno zadanie, a przy bledzie parsowania dokladnie jedna proba naprawy
    /// z [`REPAIR_INSTRUCTION`]. Drugi blad konczy zadanie — bez petli.
    /// Komunikat bledu niesie ogon surowej odpowiedzi i numer proby.
    fn converse<F>(
        &mut self,
        task: &str,
        prefix: &str,
        turns: &[Turn],
        mut send: F,
    ) -> Result<Decision, String>
    where
        F: FnMut(&Wire) -> Result<String, String>,
    {
        let first = self.wire(task, prefix, turns, false);
        let text = send(&first)?;
        let first_err = match parse_decision(&text) {
            Ok(decision) => return Ok(decision),
            Err(e) => e,
        };

        self.repairs.push(format!(
            "proba 1/2 nie sparsowala ({}), wysylam jedna probe naprawy (2/2)",
            first_err
        ));
        let second = self.wire(task, prefix, turns, true);
        match send(&second) {
            Ok(second_text) => match parse_decision(&second_text) {
                Ok(decision) => Ok(decision),
                Err(second_err) => Err(format!(
                    "{} — proba 2/2 (po naprawie); proba 1: {}; ogon odpowiedzi: {}",
                    second_err,
                    first_err,
                    tail(&second_text, TAIL_CHARS)
                )),
            },
            Err(e) => Err(format!(
                "proba 2/2 (po naprawie) nie doszla: {}; proba 1: {}",
                e, first_err
            )),
        }
    }

    /// Prob naprawy od ostatniego wywolania — agent dopisuje je do transkryptu.
    pub fn take_repairs(&mut self) -> Vec<String> {
        std::mem::take(&mut self.repairs)
    }

    /// Cialo zadania w jednym miejscu: pierwsza proba i naprawa roznia sie
    /// tylko instrukcja na koncu wiadomosci uzytkownika.
    fn wire(&self, task: &str, prefix: &str, turns: &[Turn], repair: bool) -> Wire {
        let system = system_prompt(&self.allowed, &self.skills);
        let mut user = String::new();
        user.push_str(prefix);
        user.push_str("\n\n=== ZADANIE ===\n");
        user.push_str(task);
        user.push_str("\n\n=== HISTORIA ===\n");
        if turns.is_empty() {
            user.push_str("(pusto — to pierwszy krok)\n");
        } else {
            for t in turns.iter() {
                user.push_str(&format!("[{}] {}\n", t.role, t.content));
            }
        }
        user.push_str(
            "\nOdpowiedz WYLACZNIE jednym obiektem JSON: \
             {\"thought\":\"...\",\"tool\":\"fs.read|fs.write|fs.append|notify.send\",\"args\":{...}} \
             albo {\"thought\":\"...\",\"finish\":\"podsumowanie\"}.",
        );
        if repair {
            user.push_str(REPAIR_INSTRUCTION);
        }

        let body = match self.kind {
            RemoteKind::Anthropic => json!({
                "model": self.model.clone(),
                "max_tokens": self.max_tokens,
                "system": system,
                "messages": [{ "role": "user", "content": user }]
            }),
            RemoteKind::OpenAi => json!({
                "model": self.model.clone(),
                "max_tokens": self.max_tokens,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user }
                ]
            }),
        };

        let url = match self.kind {
            RemoteKind::Anthropic => format!("{}/v1/messages", self.base_url.trim_end_matches('/')),
            RemoteKind::OpenAi => format!(
                "{}/chat/completions",
                self.base_url.trim_end_matches('/')
            ),
        };

        Wire {
            url,
            body: body.to_string(),
            headers: request_headers(self.kind, &self.api_key, &self.session),
        }
    }
}

/// Nagłówki żądania. Wszystkie idą do pliku `--config` curl (0600), nie do argv.
fn request_headers(kind: RemoteKind, api_key: &str, session: &str) -> Vec<String> {
    match kind {
        RemoteKind::Anthropic => vec![
            format!("x-api-key: {}", api_key),
            "anthropic-version: 2023-06-01".to_string(),
        ],
        RemoteKind::OpenAi => vec![
            format!("authorization: Bearer {}", api_key),
            // OC-Go: bez tego nagłówka endpoint zwraca `MissingSessionID`.
            format!("x-opencode-session: {}", session),
        ],
    }
}

fn system_prompt(allowed: &[String], skills: &[String]) -> String {
    let mut s = String::new();
    s.push_str("Jestes agentem wykonawczym. Kazdy krok to JEDNA akcja.\n");
    // Runda 17: model nie ma zegara, więc dzisiejsza data lokalna i strefa
    // muszą być w promptcie — inaczej raporty dostają wymyśloną datę.
    s.push_str(&crate::clock::date_line());
    s.push('\n');
    s.push_str("Dostepne narzedzia:\n");
    for (name, example, desc) in crate::tools::describe() {
        if !allowed.is_empty() && !allowed.iter().any(|a| a == name) {
            continue;
        }
        s.push_str(&format!("- {} — {}; argumenty: {}\n", name, desc, example));
    }
    // Runda 16: lista skilli — tylko nazwa i opis; pełna treść wchodzi do
    // kontekstu wyłącznie przez `skill.load` (albo `/skill` w TUI).
    if !skills.is_empty() {
        s.push_str("Skille (pelna tresc tylko przez skill.load):\n");
        for line in skills {
            s.push_str(line);
            s.push('\n');
        }
    }
    s.push_str("Zasady:\n");
    s.push_str("1. Sciezki tylko wzgledne, bez '..' i bez '/'. Nic poza workspace'em.\n");
    s.push_str("2. Najpierw sprawdz, potem pisz. Nie powtarzaj nieudanej akcji bez zmiany.\n");
    s.push_str("3. Gdy zadanie jest zrobione, odpowiedz polami \"finish\".\n");
    s.push_str("4. Zero komentarza poza JSON-em.\n");
    s
}

/// Limit czasu jednego zadania do modelu, w sekundach. Realny model przy
/// dluzszym przebiegu przekraczal dawne 120 s i run konczyl sie `FAILED_ERROR`.
pub const DEFAULT_TIMEOUT_SECS: u64 = 300;

/// `ARNDTAI_TIMEOUT` — sekundy, domyslnie [`DEFAULT_TIMEOUT_SECS`].
/// Wartosc niepoprawna (pusta, nie-liczba, 0) to blad klienta, nie ciche
/// podstawienie domyslnej: main konczy wtedy kodem 2 z czytelnym komunikatem.
pub fn parse_timeout(raw: Option<&str>) -> Result<u64, String> {
    let text = match raw {
        None => return Ok(DEFAULT_TIMEOUT_SECS),
        Some(t) => t.trim(),
    };
    if text.is_empty() {
        return Err(format!(
            "ARNDTAI_TIMEOUT: pusta wartosc, podaj liczbe sekund (domyslnie {})",
            DEFAULT_TIMEOUT_SECS
        ));
    }
    let secs: u64 = text.parse().map_err(|_| {
        format!(
            "ARNDTAI_TIMEOUT: '{}' nie jest liczba sekund (oczekiwano liczby calkowitej > 0)",
            text
        )
    })?;
    if secs == 0 {
        return Err("ARNDTAI_TIMEOUT: 0 jest niepoprawne, podaj liczbe sekund > 0".to_string());
    }
    Ok(secs)
}

/// Wartosc z otoczenia albo blad do pokazania uzytkownikowi (exit 2).
pub fn timeout_from_env() -> Result<u64, String> {
    let raw = env::var("ARNDTAI_TIMEOUT").ok();
    parse_timeout(raw.as_deref())
}

/// Limit dla `curl --max-time`; po walidacji w main jest zawsze poprawny,
/// wiec pozostale sciezki (TUI, MCP) nie moga sie wywrocic o te zmienna.
fn request_timeout_secs() -> u64 {
    timeout_from_env().unwrap_or(DEFAULT_TIMEOUT_SECS)
}

/// F5: naglowki (w tym klucz API) nie ida w argv — argv jest widoczne w `ps`
/// i `/proc/<pid>/cmdline`. Trafiaja do pliku konfiguracji curl (0600), podawanego
/// jako `--config <sciezka>`; w argv zostaje tylko nazwa pliku.
fn curl_argv(url: &str, config_path: &str, timeout_secs: u64) -> Vec<String> {
    vec![
        "-sS".to_string(),
        "--max-time".to_string(),
        timeout_secs.to_string(),
        "--config".to_string(),
        config_path.to_string(),
        "-X".to_string(),
        "POST".to_string(),
        url.to_string(),
        "--data-binary".to_string(),
        "@-".to_string(),
    ]
}

fn curl_config_text(headers: &[String]) -> String {
    let mut out = String::new();
    let mut all = vec!["content-type: application/json".to_string()];
    all.extend(headers.iter().cloned());
    for header in all {
        let escaped = header.replace('\\', "\\\\").replace('"', "\\\"");
        out.push_str(&format!("header = \"{}\"\n", escaped));
    }
    out
}

fn write_private_config(dir: &Path, text: &str) -> Result<PathBuf, String> {
    use std::io::Write;
    let path = dir.join(format!(
        "curl-{}-{}.conf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts
        .open(&path)
        .map_err(|e| format!("nie mozna utworzyc konfiguracji curl: {}", e))?;
    file.write_all(text.as_bytes())
        .map_err(|e| format!("nie mozna zapisac konfiguracji curl: {}", e))?;
    Ok(path)
}

/// `GET` z nagłówkami z pliku `--config` (0600) — ta sama reguła co [`http_post`]:
/// klucz API nigdy w argv. Krótki `--max-time`, bo to tylko lista modeli.
pub(crate) fn http_get(url: &str, headers: &[String], timeout_secs: u64) -> Result<Value, String> {
    let dir = std::env::temp_dir();
    let config = write_private_config(&dir, &curl_config_text(headers))?;
    let mut cmd = Command::new("curl");
    cmd.arg("-sS")
        .arg("--max-time")
        .arg(timeout_secs.to_string())
        .arg("--config")
        .arg(config.to_string_lossy().to_string())
        .arg(url);
    let output = cmd.output();
    let _ = fs::remove_file(&config);
    let output = output.map_err(|e| format!("nie mozna uruchomic curl: {}", e))?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl blad: {}", truncate(err.trim(), 300)));
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    serde_json::from_str(&text).map_err(|e| format!("odpowiedz nie jest JSON-em: {}", e))
}

/// `GET {base_url}/models` u providera OpenAI-compatible (OC-Go, OpenAI)
/// albo `GET {base_url}/v1/models` u Anthropica.
pub fn models_get(conn: &RemoteConn, timeout_secs: u64) -> Result<Value, String> {
    let base = conn.base_url.trim_end_matches('/');
    let url = match conn.kind {
        RemoteKind::Anthropic => format!("{}/v1/models", base),
        RemoteKind::OpenAi => format!("{}/models", base),
    };
    let headers = request_headers(conn.kind, &conn.api_key, &conn.session);
    http_get(&url, &headers, timeout_secs)
}

fn http_post(url: &str, body: &str, headers: &[String]) -> Result<Value, String> {
    let dir = std::env::temp_dir();
    let config = write_private_config(&dir, &curl_config_text(headers))?;
    let argv = curl_argv(url, &config.to_string_lossy(), request_timeout_secs());

    let mut cmd = Command::new("curl");
    cmd.args(&argv)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let _ = fs::remove_file(&config);
            return Err(format!("nie mozna uruchomic curl: {}", e));
        }
    };

    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| "brak stdin dla curl".to_string())?;
        stdin
            .write_all(body.as_bytes())
            .map_err(|e| format!("nie mozna wyslac zapytania: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("curl nie zakonczyl sie: {}", e))?;
    let _ = fs::remove_file(&config);

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("curl blad: {}", truncate(err.trim(), 300)));
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    serde_json::from_str(&text).map_err(|e| format!("odpowiedz nie jest JSON-em: {}", e))
}

fn parse_decision(text: &str) -> Result<Decision, String> {
    let start = text.find('{').ok_or_else(|| "brak JSON-a w odpowiedzi".to_string())?;
    let end = text.rfind('}').ok_or_else(|| "brak konca JSON-a".to_string())?;
    if end < start {
        return Err("uszkodzony JSON w odpowiedzi".to_string());
    }
    let slice = &text[start..=end];
    let value: Value = serde_json::from_str(slice)
        .map_err(|e| format!("nie mozna sparsowac odpowiedzi: {}", e))?;

    let thought = value
        .get("thought")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if let Some(summary) = value.get("finish").and_then(|v| v.as_str()) {
        return Ok(Decision {
            thought,
            action: Action::Finish {
                summary: summary.to_string(),
            },
        });
    }

    let name = value
        .get("tool")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "odpowiedz bez 'tool' ani 'finish'".to_string())?
        .to_string();
    let args = value
        .get("args")
        .cloned()
        .unwrap_or_else(|| Value::Object(Default::default()));

    Ok(Decision {
        thought,
        action: Action::Tool { name, args },
    })
}

fn truncate(s: &str, n: usize) -> String {
    let clipped: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{}...", clipped)
    } else {
        clipped
    }
}

/// Ogon surowej odpowiedzi — ostatnie `n` znakow, z `...` gdy obcieta.
/// Uciety JSON ma ogon tylko wtedy, gdy widac jego koncowke; poczatek zdania
/// (system, prompt) nic nie mowi o tym, gdzie model sie urwal.
fn tail(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.to_string();
    }
    let clipped: String = s.chars().skip(count - n).collect();
    format!("...{}", clipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-test-SECRET-123";

    fn headers() -> Vec<String> {
        vec![
            format!("x-api-key: {}", KEY),
            "anthropic-version: 2023-06-01".to_string(),
        ]
    }

    #[test]
    fn prompt_lists_only_allowed_tools() {
        let prompt = system_prompt(&["fs.read".to_string()], &[]);
        assert!(prompt.contains("fs.read"), "{}", prompt);
        assert!(
            !prompt.contains("fs.write")
                && !prompt.contains("fs.append")
                && !prompt.contains("notify.send"),
            "prompt nie moze reklamowac narzedzi spoza allowlisty: {}",
            prompt
        );
    }

    /// Runda 17: model nie ma zegara — prompt musi nieść dzisiejszą datę
    /// lokalną i strefę, żeby raporty nie dostawały wymyślonej daty.
    #[test]
    fn prompt_carries_todays_local_date_and_zone() {
        let prompt = system_prompt(&["fs.read".to_string()], &[]);
        let expected = std::process::Command::new("date")
            .arg("+%F")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        if let Some(expected) = expected {
            assert!(
                prompt.contains(&format!("Data: {}", expected)),
                "prompt bez dzisiejszej daty: {}",
                prompt.lines().take(3).collect::<Vec<_>>().join(" | ")
            );
        }
        assert!(prompt.contains("(UTC"), "strefa w promptcie: {}", prompt);
    }

    #[test]
    fn skill_list_lands_in_the_system_prompt_of_the_request() {
        let skills = vec!["- notatki: zapisuje notatki".to_string()];
        let prompt = system_prompt(&["fs.read".to_string()], &skills);
        assert!(
            prompt.contains("- notatki: zapisuje notatki"),
            "lista skilli musi byc w promptcie systemowym: {}",
            prompt
        );
        // Bez skilli nie ma pustej sekcji.
        let plain = system_prompt(&["fs.read".to_string()], &[]);
        assert!(!plain.contains("Skille ("), "{}", plain);
    }

    #[test]
    fn loaded_skill_text_lands_in_the_request_body() {
        let brain = RemoteBrain {
            kind: RemoteKind::OpenAi,
            model: "test-model".to_string(),
            base_url: "http://127.0.0.1:1".to_string(),
            api_key: "test-key".to_string(),
            session: "sesja".to_string(),
            allowed: vec!["skill.load".to_string()],
            skills: vec!["- notatki: zapisuje notatki".to_string()],
            repairs: Vec::new(),
            max_tokens: 64,
        };
        // Ten sam tekst, który agent dokłada po `/skill <nazwa>`.
        let prefix = crate::agent::render_skill_context(&[(
            "notatki".to_string(),
            "Zawsze pisz data na gorze.".to_string(),
        )]);
        let wire = brain.wire("zrob raport", &prefix, &[], false);
        let body: Value = serde_json::from_str(&wire.body).expect("cialo to JSON");
        let system = body.pointer("/messages/0/content").and_then(|v| v.as_str());
        let user = body.pointer("/messages/1/content").and_then(|v| v.as_str());
        assert!(
            system.unwrap_or("").contains("- notatki: zapisuje notatki"),
            "lista w systemowym: {}",
            wire.body
        );
        assert!(
            user.unwrap_or("").contains("Zawsze pisz data na gorze."),
            "tresc skilla w zadaniu: {}",
            wire.body
        );
        assert!(
            user.unwrap_or("").contains("--- notatki ---"),
            "sekcja z nazwa skilla: {}",
            wire.body
        );
        // Ten sam tekst nie może być płacony dwa razy w systemowym.
        assert_eq!(
            system.unwrap_or("").matches("Zawsze pisz data").count(),
            0,
            "pelna tresc tylko w zadaniu"
        );
    }

    #[test]
    fn api_key_never_appears_in_curl_argv() {
        let argv = curl_argv(
            "https://api.example/v1/messages",
            "/tmp/arndtai-curl.conf",
            300,
        );
        let joined = argv.join(" ");
        assert!(
            !joined.contains(KEY),
            "klucz nie moze trafic do argv (widoczne w ps i /proc): {}",
            joined
        );
        assert!(!joined.contains("x-api-key"), "naglowki ida poza argv");
        assert!(joined.contains("--config"), "curl musi czytac naglowki z pliku");
        assert!(!joined.contains("-H"), "zadnego -H w argv");
    }

    #[test]
    fn headers_are_written_to_private_config() {
        let text = curl_config_text(&headers());
        assert!(text.contains(KEY), "klucz musi byc w pliku konfiguracji");
        assert!(text.contains("x-api-key"));
        assert!(text.contains("content-type: application/json"));
    }

    #[test]
    fn config_file_is_owner_only_and_removed() {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-curl-{}-{}",
            std::process::id(),
            crate::read_resume_pointer(std::path::Path::new("/nonexistent")).is_none() as u8
        ));
        let _ = std::fs::create_dir_all(&dir);
        let path = write_private_config(&dir, &curl_config_text(&headers())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "plik z kluczem musi byc tylko dla wlasciciela");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn openai_request_carries_the_opencode_session_header() {
        // OC-Go bez `x-opencode-session` zwraca `MissingSessionID`.
        let out = request_headers(RemoteKind::OpenAi, KEY, "sess-abc123");
        let joined = out.join("\n");
        assert!(
            joined.contains("x-opencode-session: sess-abc123"),
            "brak naglowka sesji: {}",
            joined
        );
        assert!(joined.contains("authorization: Bearer"), "{}", joined);
        let config = curl_config_text(&out);
        assert!(
            config.contains("header = \"x-opencode-session: sess-abc123\""),
            "naglowek musi trafic do pliku config: {}",
            config
        );
        // Sekret i sesja nie moga byc widoczne w argv (ps, /proc/<pid>/cmdline).
        let argv = curl_argv(
            "https://opencode.ai/inference/go/openai/v1",
            "/tmp/c.conf",
            300,
        )
        .join(" ");
        assert!(!argv.contains("sess-abc123"), "sesja nie moze byc w argv: {}", argv);
    }

    #[test]
    fn anthropic_request_has_no_opencode_session_header() {
        let out = request_headers(RemoteKind::Anthropic, KEY, "sess-abc123");
        let joined = out.join("\n");
        assert!(!joined.contains("x-opencode-session"), "{}", joined);
        assert!(joined.contains("anthropic-version"), "{}", joined);
    }

    #[test]
    fn timeout_defaults_to_300_when_unset() {
        assert_eq!(parse_timeout(None), Ok(DEFAULT_TIMEOUT_SECS));
        assert_eq!(DEFAULT_TIMEOUT_SECS, 300);
    }

    #[test]
    fn timeout_accepts_a_positive_number_and_trims_spaces() {
        assert_eq!(parse_timeout(Some("5")), Ok(5));
        assert_eq!(parse_timeout(Some("  45  ")), Ok(45));
        assert_eq!(parse_timeout(Some("600")), Ok(600));
    }

    #[test]
    fn timeout_rejects_non_numbers_and_zero() {
        for bad in ["abc", "5s", "-1", "1.5", "", "  "] {
            let err = parse_timeout(Some(bad)).expect_err("wartosc musi byc odrzucona");
            assert!(
                err.contains("ARNDTAI_TIMEOUT"),
                "komunikat musi wskazywac zmienna: {}",
                err
            );
        }
        let zero = parse_timeout(Some("0")).expect_err("0 musi byc odrzucone");
        assert!(zero.contains("ARNDTAI_TIMEOUT"), "{}", zero);
    }

    #[test]
    fn timeout_is_passed_to_curl_as_max_time() {
        let joined = curl_argv("https://api.example/v1/messages", "/tmp/c.conf", 5).join(" ");
        assert!(
            joined.contains("--max-time 5"),
            "limit z ARNDTAI_TIMEOUT musi trafic do curl: {}",
            joined
        );
        assert!(
            !joined.contains("--max-time 120"),
            "stary limit 120 s nie moze zostac: {}",
            joined
        );
    }

    #[test]
    fn session_is_taken_from_env_or_generated_once_per_process() {
        assert_eq!(resolve_session(Some("moja-sesja")), "moja-sesja");
        assert_eq!(resolve_session(Some("  moja-sesja  ")), "moja-sesja");
        let generated = resolve_session(None);
        assert!(!generated.is_empty(), "brak ARNDTAI_SESSION musi dac identyfikator");
        let first = session_id();
        let second = session_id();
        assert_eq!(first, second, "identyfikator jest losowany raz na proces");
        assert!(!first.is_empty());
    }

    // ── Runda 12: ucięty JSON nie zabija zadania ────────────────────────────

    fn remote(kind: RemoteKind) -> RemoteBrain {
        RemoteBrain {
            kind,
            model: "test-model".to_string(),
            base_url: "https://api.example/v1".to_string(),
            api_key: KEY.to_string(),
            session: "sess-test".to_string(),
            allowed: Vec::new(),
            skills: Vec::new(),
            repairs: Vec::new(),
            max_tokens: MAX_TOKENS,
        }
    }

    const CUT: &str = "{\"thought\":\"krok\",\"tool\":\"fs.write\",\"args\":{\"path\":\"a.txt\",\"content\":\"urwane-na-koncu";
    const GOOD_FINISH: &str = "{\"thought\":\"gotowe\",\"finish\":\"zrobione\"}";
    const GOOD_TOOL: &str = "{\"thought\":\"pisze\",\"tool\":\"fs.write\",\"args\":{\"path\":\"a.txt\",\"content\":\"x\"}}";

    #[test]
    fn max_tokens_leaves_room_for_a_whole_answer() {
        // 1200 ucinalo JSON przy dluzszym `thought`/`args` (objaw z 2026-11-14).
        for kind in [RemoteKind::Anthropic, RemoteKind::OpenAi] {
            let wire = remote(kind).wire("zadanie", "", &[], false);
            let body: Value = serde_json::from_str(&wire.body).expect("cialo to JSON");
            assert_eq!(
                body["max_tokens"], 4096,
                "limit tokenow musi byc 4096, dostalem {}",
                body["max_tokens"]
            );
        }
    }

    #[test]
    fn valid_answer_costs_exactly_one_request() {
        let mut brain = remote(RemoteKind::OpenAi);
        let mut calls = 0;
        {
            let mut send = |_: &Wire| {
                calls += 1;
                Ok(GOOD_FINISH.to_string())
            };
            let decision = brain
                .converse("zadanie", "", &[], &mut send)
                .expect("poprawna odpowiedz musi wystarczyc");
            assert!(matches!(decision.action, Action::Finish { .. }));
        }
        assert_eq!(calls, 1, "poprawna odpowiedz nie moze placic drugim zadaniem");
        assert!(
            brain.take_repairs().is_empty(),
            "bez bledu parsowania nie ma proby naprawy"
        );
    }

    #[test]
    fn truncated_answer_is_repaired_once_and_the_task_goes_on() {
        let mut brain = remote(RemoteKind::OpenAi);
        let mut calls = 0;
        let mut bodies: Vec<String> = Vec::new();
        {
            let mut send = |wire: &Wire| {
                calls += 1;
                bodies.push(wire.body.clone());
                if calls == 1 {
                    Ok(CUT.to_string())
                } else {
                    Ok(GOOD_TOOL.to_string())
                }
            };
            let decision = brain
                .converse("zadanie", "", &[], &mut send)
                .expect("druga, poprawna odpowiedz ratuje krok");
            assert!(matches!(decision.action, Action::Tool { .. }));
        }
        assert_eq!(calls, 2, "dokladnie jedno zadanie naprawcze");
        // Instrukcja naprawcza tylko w drugim zadaniu; pierwsze zostaje jak bylo.
        assert!(
            !bodies[0].contains("bez prozy"),
            "pierwsze zadanie nie moze zawierac instrukcji naprawczej"
        );
        assert!(
            bodies[1].contains("bez prozy") && bodies[1].contains("WYLACZNIE poprawny JSON"),
            "drugie zadanie musi prosic o sam poprawny JSON"
        );
        let repairs = brain.take_repairs();
        assert_eq!(repairs.len(), 1, "jedna proba naprawy do transkryptu");
        assert!(
            repairs[0].contains("1/2") && repairs[0].contains("2/2"),
            "komunikat musi mowic, ktora proba: {}",
            repairs[0]
        );
    }

    #[test]
    fn two_truncated_answers_fail_with_the_tail_of_the_raw_answer() {
        let mut brain = remote(RemoteKind::OpenAi);
        let mut calls = 0;
        let err = {
            let mut send = |_: &Wire| {
                calls += 1;
                Ok(CUT.to_string())
            };
            brain
                .converse("zadanie", "", &[], &mut send)
                .expect_err("dwie uciete odpowiedzi to realny blad")
        };
        assert_eq!(calls, 2, "dokladnie jeden retry, nie petla");
        assert!(
            err.contains("proba 2/2"),
            "komunikat musi wskazac numer proby: {}",
            err
        );
        assert!(
            err.contains("ogon odpowiedzi:") && err.contains("urwane-na-koncu"),
            "komunikat musi pokazac ogon surowej odpowiedzi: {}",
            err
        );
    }

    #[test]
    fn tail_shows_the_end_not_the_beginning() {
        let long = "a".repeat(500) + "KONIEC";
        let out = tail(&long, 16);
        assert!(out.starts_with("..."), "{}", out);
        assert!(out.ends_with("KONIEC"), "{}", out);
        assert_eq!(out.chars().count(), 19);
        assert_eq!(tail("krotka", 16), "krotka");
    }

    #[test]
    fn transport_error_on_the_first_request_is_not_retried() {
        // Blad sieci to nie blad parsowania — nie ma czego naprawiac.
        let mut brain = remote(RemoteKind::Anthropic);
        let mut calls = 0;
        let err = {
            let mut send = |_: &Wire| {
                calls += 1;
                Err("curl blad: timeout".to_string())
            };
            brain
                .converse("zadanie", "", &[], &mut send)
                .expect_err("blad transportu konczy krok")
        };
        assert_eq!(calls, 1);
        assert!(err.contains("timeout"), "{}", err);
        assert!(brain.take_repairs().is_empty());
    }
}
