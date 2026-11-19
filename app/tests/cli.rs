//! Testy CLI z zewnatrz: uruchamiaja prawdziwa binarke i patrza na jej wyjscie.
//!
//! F23: `skills` i `status` mialy zaszyty poziom pamieci `lite`; teraz maja
//! respektowac `--memory <lite|pro|ultra>` tak samo jak `run`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Out {
    stdout: String,
    stderr: String,
    code: i32,
}

fn temp_dir(tag: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "arndtai-cli-{}-{}-{}",
        tag,
        std::process::id(),
        stamp
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, args: &[&str]) -> Out {
    run_env(dir, args, &[])
}

fn run_env(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Out {
    let mut cmd = Command::new(bin_path());
    cmd.args(args)
        .current_dir(dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ARNDTAI_API_KEY")
        .env_remove("ARNDTAI_BASE_URL")
        .env_remove("ARNDTAI_MODEL");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let out = cmd.output().expect("binarka arndtai powinna sie uruchomic");
    Out {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

/// Sciezka do zbudowanej binarki. `CARGO_BIN_EXE_<nazwa>` bywa niedostepne
/// przy kompilacji testu, wiec bierzemy je w czasie wykonania, a potem
/// sasiada katalogu `deps/`.
fn bin_path() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_BIN_EXE_ardntai") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent().and_then(|d| d.parent()) {
            let candidate = dir.join("ardntai");
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from("target/debug/ardntai")
}

#[test]
fn status_with_memory_pro_prints_pro() {
    let dir = temp_dir("status-pro");
    std::fs::create_dir_all(dir.join("memory")).unwrap();
    std::fs::write(dir.join("memory").join("facts.md"), "Fakt: 42.").unwrap();

    let out = run(&dir, &["status", "--memory", "pro"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Poziom pamieci: pro"),
        "status --memory pro ma wypisac 'pro', bylo:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("memory/facts.md"),
        "poziom pro widzi facts.md, bylo:\n{}",
        out.stdout
    );

    // Kontrola: bez flagi nadal dziala poziom domyslny (lite), ktory nie czyta faktow.
    let default = run(&dir, &["status"]);
    assert_eq!(default.code, 0, "stderr: {}", default.stderr);
    assert!(
        default.stdout.contains("Poziom pamieci: lite"),
        "domyslnie ma byc lite, bylo:\n{}",
        default.stdout
    );
    assert!(
        !default.stdout.contains("memory/facts.md"),
        "lite nie czyta facts.md, bylo:\n{}",
        default.stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn status_accepts_memory_equals_form() {
    let dir = temp_dir("status-eq");
    let out = run(&dir, &["status", "--memory=ultra"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert!(
        out.stdout.contains("Poziom pamieci: ultra"),
        "bylo:\n{}",
        out.stdout
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn skills_follow_the_memory_flag() {
    let dir = temp_dir("skills-pro");
    std::fs::create_dir_all(dir.join("memory")).unwrap();
    std::fs::write(dir.join("memory").join("facts.md"), "Fakt: 7.").unwrap();

    let lite = run(&dir, &["skills"]);
    assert_eq!(lite.code, 0, "stderr: {}", lite.stderr);
    assert!(
        lite.stdout.contains("Poziom pamieci: lite"),
        "bylo:\n{}",
        lite.stdout
    );

    let pro = run(&dir, &["skills", "--memory", "pro"]);
    assert_eq!(pro.code, 0, "stderr: {}", pro.stderr);
    assert!(
        pro.stdout.contains("Poziom pamieci: pro"),
        "skills --memory pro ma wypisac 'pro', bylo:\n{}",
        pro.stdout
    );
    assert!(
        pro.stdout.contains("memory/facts.md"),
        "poziom pro widzi facts.md, bylo:\n{}",
        pro.stdout
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn unknown_memory_level_is_rejected() {
    let dir = temp_dir("status-bad");
    for cmd in ["status", "skills"] {
        let out = run(&dir, &[cmd, "--memory", "bogus"]);
        assert_eq!(out.code, 2, "{} ma zwrocic kod 2, stdout:\n{}", cmd, out.stdout);
        assert!(
            out.stderr.contains("lite, pro, ultra"),
            "{} ma powiedziec, jakie poziomy sa dozwolone, stderr:\n{}",
            cmd,
            out.stderr
        );
        assert!(
            !out.stdout.contains("Poziom pamieci:"),
            "{} nie moze wypisac stanu przy blednym argumencie, stdout:\n{}",
            cmd,
            out.stdout
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Runda 12: ucięty JSON nie zabija zadania ────────────────────────────────
//
// Prawdziwy model zastepuje serwer HTTP na localhost: liczy zadania i oddaje
// odpowiedzi po kolei, wiec test widzi, ile razy agent naprawde zapytal model.

const CUT_ANSWER: &str =
    "{\"thought\":\"pisze\",\"tool\":\"fs.write\",\"args\":{\"path\":\"a.txt\",\"content\":\"urwane";
const CUT_ANSWER_2: &str = "{\"thought\":\"jeszcze-raz\"";
const FINISH_ANSWER: &str = "{\"thought\":\"gotowe\",\"finish\":\"notatka zapisana\"}";

/// Odpowiedź atrapy zależna od żądania: `stream: true` dostaje SSE
/// (`data: {...delta...}`, potem `data: [DONE]`), a próba bez strumienia
/// zwykły JSON. Ten sam `answer` w obu wariantach, więc testy liczą żądania,
/// a nie format — tak jak w rundzie 21 czyta je `provider.rs`.
fn sse_or_json(request: &str, answer: &str) -> (String, &'static str) {
    if request.contains("\"stream\":true") {
        let chunk = serde_json::json!({
            "choices": [{ "delta": { "content": answer } }]
        })
        .to_string();
        (
            format!("data: {}\n\ndata: [DONE]\n\n", chunk),
            "text/event-stream",
        )
    } else {
        let body = serde_json::json!({
            "choices": [{ "message": { "content": answer } }]
        })
        .to_string();
        (body, "application/json")
    }
}

fn fake_model(answers: Vec<String>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("wolny port na localhost");
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&hits);
    std::thread::spawn(move || {
        for answer in answers {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => return,
            };
            let request = read_http_body(&mut stream);
            let (body, content_type) = sse_or_json(&request, &answer);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                content_type,
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    (format!("http://{}", addr), hits)
}


/// Czyta całe żądanie HTTP i zwraca jego ciało (JSON). Wariant `capturing`
/// potrzebuje treści, żeby sprawdzić, co naprawdę poszło do providera.
fn read_http_body(stream: &mut std::net::TcpStream) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = match stream.read(&mut chunk) {
            Ok(0) => return String::new(),
            Ok(n) => n,
            Err(_) => return String::new(),
        };
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_bytes(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            while buf.len() < pos + 4 + len {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                }
            }
            let body = &buf[pos + 4..];
            return String::from_utf8_lossy(body).to_string();
        }
    }
}

/// `fake_model`, który dodatkowo zapisuje ciała żądań — test sprawdza
/// `max_tokens` z `.agent/config` na lokalnym atrapie providera.
fn fake_model_capturing(
    answers: Vec<String>,
) -> (String, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("wolny port na localhost");
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let bodies: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let counter = Arc::clone(&hits);
    let log = Arc::clone(&bodies);
    std::thread::spawn(move || {
        for answer in answers {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => return,
            };
            let body = read_http_body(&mut stream);
            log.lock().unwrap().push(body.clone());
            let (reply, content_type) = sse_or_json(&body, &answer);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                content_type,
                reply.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });
    (format!("http://{}", addr), hits, bodies)
}

/// Czyta naglowki i cale cialo zadania, zeby curl dostal odpowiedz dopiero po
/// wyslaniu JSON-a.
fn read_http_request(stream: &mut std::net::TcpStream) -> std::io::Result<()> {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = find_bytes(&buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok())
                .unwrap_or(0);
            let mut left = len.saturating_sub(buf.len() - (pos + 4));
            while left > 0 {
                let n = stream.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                left = left.saturating_sub(n);
            }
            return Ok(());
        }
    }
}

fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Caly transkrypt sesji z `.agent/sessions` sklejony w jeden tekst.
fn transcript(dir: &Path) -> String {
    let mut text = String::new();
    if let Ok(entries) = std::fs::read_dir(dir.join(".agent").join("sessions")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    text.push_str(&content);
                }
            }
        }
    }
    text
}

// ── Runda 13: podmiana binarki w trakcie pracy ──────────────────────────────
//
// Proces, ktorego plik podmieniono, widzi wlasna sciezke z sufiksem
// `" (deleted)"`. Perimetr MCP musi wtedy trafic na prawdziwy plik (albo na
// `ARNDTAI_SELF`), a nie wywalic sie na `No such file or directory`.

/// Uruchamia wskazana binarke (kopie), nie `bin_path()`.
fn run_bin(bin: &Path, dir: &Path, args: &[&str]) -> Out {
    let out = Command::new(bin)
        .args(args)
        .current_dir(dir)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ARNDTAI_API_KEY")
        .env_remove("ARNDTAI_BASE_URL")
        .env_remove("ARNDTAI_SELF")
        .output()
        .expect("binarka arndtai powinna sie uruchomic");
    Out {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        code: out.status.code().unwrap_or(-1),
    }
}

#[test]
fn replaced_binary_still_has_a_perimeter() {
    let dir = temp_dir("self-replace");
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();

    // Kopia pod wlasna nazwa: to jej sciezke proces uznaje za `current_exe`.
    let copy = dir.join("ardntai");
    std::fs::copy(bin_path(), &copy).expect("kopia binarki");

    let first = run_bin(
        &copy,
        &ws,
        &["run", "write a.txt :: alfa", "--provider", "mock", "-q"],
    );
    assert_eq!(
        first.code, 0,
        "pierwsze zadanie ma przejsc, stderr:\n{}",
        first.stderr
    );
    let before = transcript(&ws);
    assert!(
        before.contains("\"kind\":\"tool_result\""),
        "perimetr musi wykonac narzedzie:\n{}",
        before
    );

    // Podmiana binarki tak jak `cargo build`: kopia na bok, `rm`, nowy plik.
    let aside = dir.join("ardntai.bak");
    std::fs::copy(&copy, &aside).unwrap();
    std::fs::remove_file(&copy).unwrap();
    std::fs::copy(&aside, &copy).unwrap();

    // Ta sama sesja (`--resume`), wiec licznik i transkrypt ida dalej.
    let second = run_bin(
        &copy,
        &ws,
        &[
            "run",
            "write b.txt :: beta",
            "--provider",
            "mock",
            "--resume",
            "-q",
        ],
    );
    let both = format!("{}\n{}", second.stdout, second.stderr);
    assert_eq!(second.code, 0, "drugie zadanie ma przejsc:\n{}", both);
    assert!(
        !both.contains("No such file or directory"),
        "podmiana binarki nie moze zabrac perimetru:\n{}",
        both
    );
    assert!(
        !both.contains("perimeter MCP nie wystartowal"),
        "perimetr musi wstac po podmianie:\n{}",
        both
    );

    let after = transcript(&ws);
    assert!(
        after.contains("\"status\":\"DONE\""),
        "druga tura musi skonczyc sie DONE:\n{}",
        after
    );
    assert!(
        after.contains("b.txt"),
        "narzedzie drugiej tury tez ma zadzialac:\n{}",
        after
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Gdy sciezka z `current_exe` nie istnieje (plik naprawde znikl), proces ma
/// uzyc `ARNDTAI_SELF` i dalej miec perimetr.
#[test]
fn deleted_binary_falls_back_to_self_env() {
    let dir = temp_dir("self-env");
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();

    let copy = dir.join("ardntai");
    std::fs::copy(bin_path(), &copy).expect("kopia binarki");
    // Nowa lokalizacja poza katalogiem kopii; `ARNDTAI_SELF` wskazuje na nia.
    let fresh = dir.join("nowa").join("ardntai");
    std::fs::create_dir_all(fresh.parent().unwrap()).unwrap();
    std::fs::copy(bin_path(), &fresh).unwrap();

    // Pierwszy przebieg z kopii, zeby sesja istniala.
    let first = run_bin(
        &copy,
        &ws,
        &["run", "write a.txt :: alfa", "--provider", "mock", "-q"],
    );
    assert_eq!(first.code, 0, "stderr:\n{}", first.stderr);

    let out = Command::new(&copy)
        .args([
            "run",
            "write c.txt :: gamma",
            "--provider",
            "mock",
            "--resume",
            "-q",
        ])
        .current_dir(&ws)
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ARNDTAI_API_KEY")
        .env_remove("ARNDTAI_BASE_URL")
        .env("ARNDTAI_SELF", &fresh)
        .output()
        .expect("binarka powinna sie uruchomic");
    let both = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "fallback ma dzialac:\n{}", both);
    assert!(
        !both.contains("perimeter MCP nie wystartowal"),
        "ARNDTAI_SELF musi ratowac perimetr:\n{}",
        both
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn truncated_answer_is_repaired_and_the_task_finishes() {
    let dir = temp_dir("repair-ok");
    let (base, hits) = fake_model(vec![CUT_ANSWER.to_string(), FINISH_ANSWER.to_string()]);
    let out = run_env(
        &dir,
        &["run", "zapisz notatke", "--provider", "openai", "--no-mcp", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(
        out.code, 0,
        "zadanie ma sie skonczyc DONE, stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "jedno zadanie + dokladnie jedna proba naprawy"
    );
    let text = transcript(&dir);
    assert!(
        text.contains("\"kind\":\"repair\""),
        "transkrypt ma pokazac probe naprawy:\n{}",
        text
    );
    assert!(
        text.contains("\"status\":\"DONE\""),
        "zadanie ma konczyc sie DONE:\n{}",
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn valid_first_answer_does_not_cost_a_second_request() {
    let dir = temp_dir("repair-none");
    let (base, hits) = fake_model(vec![FINISH_ANSWER.to_string()]);
    let out = run_env(
        &dir,
        &["run", "zapisz notatke", "--provider", "openai", "--no-mcp", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(out.code, 0, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
    assert_eq!(hits.load(Ordering::SeqCst), 1, "bez bledu nie ma retry");
    let text = transcript(&dir);
    assert!(
        !text.contains("\"kind\":\"repair\""),
        "poprawna odpowiedz nie moze dopisywac naprawy:\n{}",
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_truncated_answers_fail_with_the_tail_of_the_answer() {
    let dir = temp_dir("repair-fail");
    let (base, hits) = fake_model(vec![CUT_ANSWER.to_string(), CUT_ANSWER_2.to_string()]);
    let out = run_env(
        &dir,
        &["run", "zapisz notatke", "--provider", "openai", "--no-mcp"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(
        hits.load(Ordering::SeqCst),
        2,
        "dokladnie jedna proba naprawy, nie petla"
    );
    assert_eq!(out.code, 1, "realny blad konczy sie kodem 1");
    assert!(
        out.stdout.contains("FAILED_ERROR"),
        "status zostaje FAILED_ERROR, stdout:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("proba 2/2") && out.stdout.contains("ogon odpowiedzi:"),
        "komunikat musi miec numer proby i ogon odpowiedzi, stdout:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("jeszcze-raz"),
        "ogon ma pokazac koncowke drugiej odpowiedzi, stdout:\n{}",
        out.stdout
    );
    let text = transcript(&dir);
    assert!(
        text.contains("\"kind\":\"error\""),
        "blad musi trafic do transkryptu:\n{}",
        text
    );
    assert!(
        text.contains("ogon odpowiedzi:") && text.contains("jeszcze-raz"),
        "ogon odpowiedzi musi byc w JSONL:\n{}",
        text
    );
    assert!(
        text.contains("\"status\":\"FAILED_ERROR\""),
        "status sesji zostaje FAILED_ERROR:\n{}",
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Runda 15: limit odpowiedzi z `.agent/config` ───────────────────────────

/// `max_tokens = 1024` w configu naprawdę idzie w żądaniu do providera.
#[test]
fn config_max_tokens_goes_into_the_request() {
    let dir = temp_dir("tokens-1024");
    std::fs::create_dir_all(dir.join(".agent")).unwrap();
    std::fs::write(
        dir.join(".agent").join("config"),
        "max_tokens = 1024\ntheme = jasny\n",
    )
    .unwrap();
    let (base, hits, bodies) = fake_model_capturing(vec![FINISH_ANSWER.to_string()]);
    let out = run_env(
        &dir,
        &["run", "zapisz notatke", "--provider", "openai", "--no-mcp", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(
        out.code, 0,
        "zadanie ma sie skonczyc DONE, stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    let body = bodies.lock().unwrap().first().cloned().unwrap_or_default();
    let json: serde_json::Value = serde_json::from_str(&body).expect("cialo to JSON");
    assert_eq!(
        json["max_tokens"], 1024,
        "limit z configu musi byc w zadaniu, cialo: {body}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Bez wpisu w configu zostaje dotychczasowe 4096 — testy rundy 12/14 się nie
/// zmieniają.
#[test]
fn default_max_tokens_stays_at_4096() {
    let dir = temp_dir("tokens-default");
    let (base, _hits, bodies) = fake_model_capturing(vec![FINISH_ANSWER.to_string()]);
    let out = run_env(
        &dir,
        &["run", "zapisz notatke", "--provider", "openai", "--no-mcp", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(out.code, 0, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
    let body = bodies.lock().unwrap().first().cloned().unwrap_or_default();
    let json: serde_json::Value = serde_json::from_str(&body).expect("cialo to JSON");
    assert_eq!(json["max_tokens"], 4096, "cialo: {body}");
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Runda 16: skille ────────────────────────────────────────────────────────

/// Dwa skille w workspace: jeden poprawny, jeden z zepsutym nagłówkiem.
fn seed_skills(dir: &Path) {
    let raport = dir.join("skills").join("raport");
    std::fs::create_dir_all(&raport).unwrap();
    std::fs::write(
        raport.join("SKILL.md"),
        "---\nname: raport\ndescription: dzienny raport z postepu\nwhen: gdy trzeba podsumowac dzien\n---\nZawsze podaj trzy liczby.\n",
    )
    .unwrap();
    let zepsuty = dir.join("skills").join("zepsuty");
    std::fs::create_dir_all(&zepsuty).unwrap();
    std::fs::write(
        zepsuty.join("SKILL.md"),
        "---\nname: zepsuty\nto nie jest yaml\n---\ntresc\n",
    )
    .unwrap();
}

/// `ardntai skills` pokazuje listę z katalogiem źródłowym i znacznikiem
/// zepsutego nagłówka (ten sam format co modal `/skills`).
#[test]
fn skills_command_lists_discovered_skills_with_their_source() {
    let dir = temp_dir("skills-cli");
    seed_skills(&dir);
    let cfg_dir = dir.join("config");
    std::fs::create_dir_all(&cfg_dir).unwrap();
    let out = run_env(
        &dir,
        &["skills"],
        &[
            ("XDG_CONFIG_HOME", cfg_dir.to_str().unwrap()),
            ("ARNDTAI_APP_DIR", dir.join("app").to_str().unwrap()),
        ],
    );
    assert_eq!(out.code, 0, "stderr:\n{}", out.stderr);
    let text = &out.stdout;
    assert!(text.contains("Skille (2)"), "dwa skille na liscie:\n{}", text);
    assert!(text.contains("raport"), "{}", text);
    assert!(
        text.contains("dzienny raport z postepu"),
        "opis z naglowka:\n{}",
        text
    );
    assert!(
        text.contains("zrodlo: workspace"),
        "katalog zrodlowy musi byc widoczny:\n{}",
        text
    );
    assert!(
        text.contains("kiedy: gdy trzeba podsumowac dzien"),
        "pole when z naglowka:\n{}",
        text
    );
    assert!(
        text.contains("blad naglowka"),
        "zepsuty naglowek oznaczony, nie wywalony:\n{}",
        text
    );
    assert!(
        text.contains("Lista w promptcie: 1 pozycji"),
        "zepsuty nie wchodzi do listy w promptcie:\n{}",
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

const SKILL_ANSWER: &str =
    "{\"thought\":\"laduje skill\",\"tool\":\"skill.load\",\"args\":{\"name\":\"raport\"}}";
const SKILL_404_ANSWER: &str =
    "{\"thought\":\"zly skill\",\"tool\":\"skill.load\",\"args\":{\"name\":\"nie-ma\"}}";

/// Runda 17: `skill.load` przez MCP raportowal `0 bajtow`, bo klient czytal
/// tylko `size`/`bytes_written`. Ten test idzie prawdziwym perimetrem MCP
/// (bez `--no-mcp`) i patrzy na linie `[OBSERVE]`.
#[test]
fn skill_load_reports_the_real_byte_count_through_mcp() {
    let dir = temp_dir("skills-bytes");
    seed_skills(&dir);
    let (base, _hits) = fake_model(vec![SKILL_ANSWER.to_string(), FINISH_ANSWER.to_string()]);
    let out = run_env(
        &dir,
        &["run", "zrob raport", "--provider", "openai"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(
        out.code, 0,
        "stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    let expected = format!(
        "skill.load ok=true ({} bajtow)",
        "Zawsze podaj trzy liczby.".len()
    );
    assert!(
        out.stdout.contains(&expected),
        "licznik musi pokazac rozmiar tresci ({expected}):\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("skill.load ok=true (0 bajtow)"),
        "zero bajtow to objaw naprawionego bledu:\n{}",
        out.stdout
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Lista skilli w promptcie systemowym, a treść — dopiero po `skill.load`.
/// Sprawdzane na lokalnym atrapie providera: dwa żądania, oba ciała.
#[test]
fn skill_list_and_loaded_content_reach_the_request() {
    let dir = temp_dir("skills-e2e");
    seed_skills(&dir);
    let (base, hits, bodies) =
        fake_model_capturing(vec![SKILL_ANSWER.to_string(), FINISH_ANSWER.to_string()]);
    let out = run_env(
        &dir,
        &[
            "run",
            "zrob raport",
            "--provider",
            "openai",
            "--no-mcp",
            "-q",
        ],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(
        out.code, 0,
        "stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert_eq!(hits.load(Ordering::SeqCst), 2, "jedno wywolanie narzedzia + finish");
    let bodies = bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 2);
    assert!(
        bodies[0].contains("- raport: dzienny raport z postepu"),
        "lista `- nazwa: opis` musi byc w pierwszym zadaniu:\n{}",
        bodies[0]
    );
    assert!(
        !bodies[0].contains("Zawsze podaj trzy liczby."),
        "pelna tresc nie moze isc bez prosby:\n{}",
        bodies[0]
    );
    assert!(
        bodies[1].contains("Zawsze podaj trzy liczby."),
        "po skill.load tresc jest w zadaniu:\n{}",
        bodies[1]
    );
    let text = transcript(&dir);
    assert!(
        text.contains("\"tool\":\"skill.load\""),
        "wywolanie w transkrypcie:\n{}",
        text
    );
    assert!(
        text.contains("\"status\":\"DONE\""),
        "zadanie konczy sie czysto:\n{}",
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `skill.load` poza listą to `NOT_FOUND` — bramka perimetru, nie crash.
#[test]
fn skill_load_outside_the_list_is_not_found() {
    let dir = temp_dir("skills-404");
    std::fs::create_dir_all(dir.join("skills")).unwrap();
    let (base, _hits) = fake_model(vec![
        SKILL_404_ANSWER.to_string(),
        FINISH_ANSWER.to_string(),
    ]);
    let out = run_env(
        &dir,
        &[
            "run",
            "sprobuj nie-ma",
            "--provider",
            "openai",
            "--no-mcp",
            "-q",
        ],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    assert_eq!(out.code, 0, "stdout:\n{}\nstderr:\n{}", out.stdout, out.stderr);
    let text = transcript(&dir);
    assert!(
        text.contains("NOT_FOUND"),
        "brak skilla na liscie musi dac NOT_FOUND:\n{}",
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── M1: utrata procesu w środku pracy (kill -9) ────────────────────────────
//
// Sygnał nie daje procesowi szansy na posprzątanie: ginie bufor, transakcja
// i perimetr. Kontrakt (`docs/contracts/events.md`) mówi, że krok `proposed`
// bez wyniku po wznowieniu jest `abandoned` i nie wykonuje się drugi raz,
// a krok `done` idzie z zapisanego wyniku. Do tej pory pilnowały tego testy
// jednostkowe; te dwa widzą to na prawdziwej binarce i prawdziwym SIGKILL.

/// Uruchamia binarkę jako dziecko (bez czekania), z tym samym czyszczeniem
/// środowiska co `run_env`, żeby żaden prawdziwy klucz nie wyciekł do testu.
fn spawn_bin(
    dir: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> std::process::Child {
    let mut cmd = Command::new(bin_path());
    cmd.args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ARNDTAI_API_KEY")
        .env_remove("ARNDTAI_BASE_URL")
        .env_remove("ARNDTAI_MODEL");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.spawn().expect("binarka arndtai powinna sie uruchomic")
}

/// To samo co `run_env`, ale z limitem czasu: proces, który zawisł na
/// niepominiętym kroku, daje czytelny błąd zamiast wiszącego `cargo test`.
fn run_env_within(
    dir: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    limit: std::time::Duration,
) -> Out {
    let mut cmd = Command::new(bin_path());
    cmd.args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("OPENAI_API_KEY")
        .env_remove("ARNDTAI_API_KEY")
        .env_remove("ARNDTAI_BASE_URL")
        .env_remove("ARNDTAI_MODEL");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("binarka arndtai powinna sie uruchomic");
    let started = std::time::Instant::now();
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                return Out {
                    stdout,
                    stderr,
                    code: status.code().unwrap_or(-1),
                };
            }
            None if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "proces nie skonczyl w {:?} — krok nie zostal pominiety",
                    limit
                );
            }
            None => std::thread::sleep(std::time::Duration::from_millis(20)),
        }
    }
}

/// Czeka, aż transkrypt sesji zawiera fragment. Panikuje po `timeout`, bo
/// test, który wisi, nie mówi nic o przyczynie.
fn wait_for_transcript(dir: &Path, needle: &str, timeout: std::time::Duration) {
    let started = std::time::Instant::now();
    loop {
        if transcript(dir).contains(needle) {
            return;
        }
        if started.elapsed() > timeout {
            panic!(
                "transkrypt nie zawiera {:?} po {:?}:\n{}",
                needle,
                timeout,
                transcript(dir)
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Model, który odpowiada na pierwsze żądanie, a na drugie nie odpowiada
/// wcale — proces zostaje zablokowany na czytaniu odpowiedzi providera.
/// Tak wygląda prawdziwa utrata procesu w środku tury.
fn fake_model_then_hang(answer: String) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("wolny port na localhost");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let request = read_http_body(&mut stream);
            let (body, content_type) = sse_or_json(&request, &answer);
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                content_type,
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body.as_bytes());
            let _ = stream.flush();
        }
        if let Ok((mut stream, _)) = listener.accept() {
            let _ = read_http_request(&mut stream);
            // Milczy do konca testu: proces czeka na odpowiedz, az go zabijemy.
            std::thread::sleep(std::time::Duration::from_secs(30));
            drop(stream);
        }
    });
    format!("http://{}", addr)
}

/// Krok `done` nie może wykonać się drugi raz po restarcie: plik zostaje
/// z jedną porcją treści, a krok wraca z zapisanego wyniku (`cached`).
#[test]
fn killing_the_process_while_the_model_thinks_does_not_repeat_the_step() {
    let dir = temp_dir("kill-model");
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let append = "{\"thought\":\"dopisuje\",\"tool\":\"fs.append\",\"args\":{\"path\":\"a.txt\",\"content\":\"raz\"}}";
    let base = fake_model_then_hang(append.to_string());
    let mut child = spawn_bin(
        &ws,
        &["run", "dopisz raz", "--provider", "openai", "--no-mcp", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );

    wait_for_transcript(
        &ws,
        "\"kind\":\"tool_result\"",
        std::time::Duration::from_secs(10),
    );
    // Krok jest `done`, a proces czeka na drugą odpowiedź modelu — tu ginie.
    child.kill().expect("SIGKILL na dziecku");
    let _ = child.wait();
    assert_eq!(
        std::fs::read_to_string(ws.join("a.txt")).unwrap(),
        "raz",
        "efekt kroku jest zapisany przed zabiciem"
    );

    // Plik chowamy: gdyby wznowienie wykonało krok ponownie, `a.txt` powstałby
    // od nowa. To jedyny sposób, by odróżnić powtórkę od zapisanego wyniku.
    std::fs::rename(ws.join("a.txt"), ws.join("a.txt.bak")).unwrap();

    let (resume_base, _) = fake_model(vec![append.to_string(), FINISH_ANSWER.to_string()]);
    let out = run_env_within(
        &ws,
        &[
            "run",
            "dopisz raz",
            "--provider",
            "openai",
            "--no-mcp",
            "--resume",
            "-q",
        ],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", resume_base.as_str()),
        ],
        std::time::Duration::from_secs(30),
    );
    assert_eq!(
        out.code, 0,
        "wznowienie ma się skończyć DONE, stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert!(
        !ws.join("a.txt").exists(),
        "krok `done` nie może wykonać efektu drugi raz (at-most-once):\n{}",
        transcript(&ws)
    );
    assert_eq!(
        std::fs::read_to_string(ws.join("a.txt.bak")).unwrap(),
        "raz",
        "efekt z pierwszego biegu zostaje nietknięty"
    );
    assert!(
        transcript(&ws).contains("\"kind\":\"tool_result_cached\""),
        "wznowienie ma wziąć wynik z zapisanego kroku:\n{}",
        transcript(&ws)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Ta sama próba przez perimetr MCP (ścieżka domyślna): serwer trzyma własną
/// bramkę na tej samej `state.db`, więc po SIGKILL wznowiony krok `done`
/// wraca z zapisanym wynikiem, a nie drugim efektem.
#[test]
fn mcp_resume_replays_a_done_step_without_reexecuting_it() {
    let dir = temp_dir("kill-model-mcp");
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let append = "{\"thought\":\"dopisuje\",\"tool\":\"fs.append\",\"args\":{\"path\":\"a.txt\",\"content\":\"raz\"}}";
    let base = fake_model_then_hang(append.to_string());
    let mut child = spawn_bin(
        &ws,
        &["run", "dopisz raz", "--provider", "openai", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    wait_for_transcript(
        &ws,
        "\"kind\":\"tool_result\"",
        std::time::Duration::from_secs(10),
    );
    child.kill().expect("SIGKILL na dziecku");
    let _ = child.wait();
    assert_eq!(
        std::fs::read_to_string(ws.join("a.txt")).unwrap(),
        "raz",
        "efekt kroku jest zapisany przed zabiciem"
    );
    std::fs::rename(ws.join("a.txt"), ws.join("a.txt.bak")).unwrap();

    let (resume_base, _) = fake_model(vec![append.to_string(), FINISH_ANSWER.to_string()]);
    let out = run_env_within(
        &ws,
        &["run", "dopisz raz", "--provider", "openai", "--resume", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", resume_base.as_str()),
        ],
        std::time::Duration::from_secs(30),
    );
    assert_eq!(
        out.code, 0,
        "wznowienie ma się skończyć DONE, stdout:\n{}\nstderr:\n{}",
        out.stdout, out.stderr
    );
    assert!(
        !ws.join("a.txt").exists(),
        "perimetr MCP nie może wykonać efektu drugi raz:\n{}",
        transcript(&ws)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Krok `proposed` bez wyniku (proces zabity w środku narzędzia) po
/// wznowieniu jest `abandoned`: zero powtórzenia efektu, a tura idzie dalej.
/// FIFO jest tu narzędziem: `fs.read` otwiera je do czytania i czeka na
/// pisarza, więc proces naprawdę ginie „w środku" narzędzia, nie po nim.
#[test]
fn killing_the_process_inside_the_tool_leaves_the_step_abandoned() {
    let dir = temp_dir("kill-tool");
    let ws = dir.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let fifo = ws.join("cel.txt");
    let made = Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo powinien byc dostepny");
    assert!(made.success(), "mkfifo musi utworzyc fifo");

    let (base, _) = fake_model(vec![
        "{\"thought\":\"czytam\",\"tool\":\"fs.read\",\"args\":{\"path\":\"cel.txt\"}}".to_string(),
    ]);
    let mut child = spawn_bin(
        &ws,
        &["run", "zapisz", "--provider", "openai", "--no-mcp", "-q"],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", base.as_str()),
        ],
    );
    wait_for_transcript(
        &ws,
        "\"kind\":\"tool_call\"",
        std::time::Duration::from_secs(10),
    );
    // Zaraz po `tool_call` narzędzie blokuje się na FIFO; krok jest `proposed`.
    child.kill().expect("SIGKILL na dziecku");
    let _ = child.wait();

    // Ten sam krok jeszcze raz, a potem koniec: przerwany `proposed` musi
    // zostać pominięty, zamiast czekać drugi raz na FIFO.
    let (resume_base, _) = fake_model(vec![
        "{\"thought\":\"czytam\",\"tool\":\"fs.read\",\"args\":{\"path\":\"cel.txt\"}}".to_string(),
        FINISH_ANSWER.to_string(),
    ]);
    let out = run_env_within(
        &ws,
        &[
            "run",
            "zapisz",
            "--provider",
            "openai",
            "--no-mcp",
            "--resume",
            "-q",
        ],
        &[
            ("ARNDTAI_API_KEY", "test-key"),
            ("ARNDTAI_BASE_URL", resume_base.as_str()),
        ],
        std::time::Duration::from_secs(30),
    );
    let both = format!("{}\n{}", out.stdout, out.stderr);
    assert_eq!(out.code, 0, "wznowienie ma przejsc:\n{}", both);
    let text = transcript(&ws);
    assert!(
        text.contains("\"kind\":\"tool_result_skipped\""),
        "krok przerwany przed wynikiem ma byc pominiety:\n{}",
        text
    );
    assert!(
        text.contains("crash_before_result"),
        "pominiecie ma powod `crash_before_result`:\n{}",
        text
    );
    assert!(
        !text.contains("\"kind\":\"tool_result\""),
        "zabity krok nie moze wykonac sie drugi raz:\n{}",
        text
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--version` i `-V` musza wypisac wersje z Cargo.toml i skonczyc kodem 0,
/// zamiast potraktowac flage jak tresc zadania (tak bylo do 2026-11-19).
#[test]
fn version_flags_print_the_crate_version() {
    let dir = temp_dir("version");
    for flag in ["--version", "-V"] {
        let out = run(&dir, &[flag]);
        assert_eq!(out.code, 0, "{flag} stderr: {}", out.stderr);
        let text = format!("{}{}", out.stdout, out.stderr);
        assert!(
            text.contains(env!("CARGO_PKG_VERSION")),
            "{flag} ma pokazac wersje {}: {text}",
            env!("CARGO_PKG_VERSION")
        );
        assert!(
            !text.contains("Zadanie:"),
            "{flag} nie moze byc tresciа zadania: {text}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--screenshot` to komenda, ktora Craft Anything uruchamia po kazdym
/// budowaniu: musi zapisac prawdziwy, niepusty PNG i skonczyc sie kodem 0
/// bez terminala i bez okna. Sprawdzamy tez, ze po zrzucie nie zostaje
/// jednorazowy workspace `.agent/screenshot-ws`.
#[test]
fn screenshot_option_writes_a_non_blank_png() {
    let dir = temp_dir("screenshot");
    let out = run(&dir, &["--screenshot", "ekran.png"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let bytes = std::fs::read(dir.join("ekran.png")).expect("zrzut powinien powstac");
    assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    assert_eq!((width, height), (960, 608), "domyslny kadr to 120x38 znakow");
    assert!(bytes.len() > 10_000, "zrzut nie moze byc pusty");

    assert!(
        !dir.join(".agent").join("screenshot-ws").exists(),
        "workspace zrzutu ma byc usuniety"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Rozmiar kadru z argumentu (`83x38`) i blad bez sciezki — oba wchodza przez
/// to samo wejscie CLI, wiec testuja `size_arg` oraz kod wyjscia 2.
#[test]
fn screenshot_option_takes_a_custom_size_and_rejects_a_missing_path() {
    let dir = temp_dir("screenshot-size");
    let out = run(&dir, &["--screenshot", "waski.png", "83x38"]);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let bytes = std::fs::read(dir.join("waski.png")).expect("zrzut powinien powstac");
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    assert_eq!((width, height), (83 * 8, 38 * 16), "83 kolumny x 38 wierszy");

    let missing = run(&dir, &["--screenshot"]);
    assert_eq!(missing.code, 2, "brak sciezki to blad uzycia");
    assert!(
        missing.stderr.contains("wymaga sciezki"),
        "stderr: {}",
        missing.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}
