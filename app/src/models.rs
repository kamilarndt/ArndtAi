//! Metadane modeli: `GET {base_url}/models` u providera OpenAI-compatible
//! (runda 14).
//!
//! Lista modeli jest potrzebna w dwóch miejscach: licznik kontekstu w prawym
//! górnym roku (`<zużyte> / <limit.context>`) i picker `/model`. Provider
//! wystawia ją w `GET /models`:
//!
//! ```json
//! {"id": "mimo-v2.6-flash", "limit": {"context": 1048576, "output": 131072},
//!  "cost": {"input": 0.05, "output": 0.2},
//!  "capabilities": {"tools": {"supported": true}}}
//! ```
//!
//! Pobieramy **raz na start sesji**, z cache w `.agent/models.json` i TTL 1 h.
//! Brak sieci albo klucza → cache (także przeterminowany), a gdy go nie ma —
//! pusta lista, czyli `?` w liczniku. Nic nie jest zgadywane: brak `limit`
//! w odpowiedzi to brak limitu, nie domyślne 200K.

use crate::provider::RemoteConn;
use serde_json::{json, Map, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Ile ważny jest cache listy modeli (sekundy).
pub const CACHE_TTL_SECS: u64 = 3600;
/// Ile czekamy na `GET /models`, zanim uznamy, że sieci nie ma (sekundy).
pub const FETCH_TIMEOUT_SECS: u64 = 5;
const CACHE_FILE: &str = "models.json";
const CONFIG_FILE: &str = "config";

// ── Model i katalog ─────────────────────────────────────────────────────────

/// Jeden model z `/models`: identyfikator, nazwa, limity i cena.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub id: String,
    /// Nazwa do pokazania; gdy provider jej nie daje — `id`.
    pub name: String,
    /// `limit.context` — okno kontekstu w tokenach. `None` = provider nie podał.
    pub context: Option<u64>,
    /// `limit.output` — limit odpowiedzi; sufit dla `max_tokens`.
    pub output: Option<u64>,
    /// `cost.input` — cena za 1M tokenów wejścia (konwencja providera).
    pub price_in: Option<f64>,
    /// `cost.output` — cena za 1M tokenów wyjścia.
    pub price_out: Option<f64>,
}

/// Skąd pochodzi lista — widoczne w `/model` i w notatce RE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Świeża odpowiedź `GET /models`.
    Network,
    /// Cache młodszy niż [`CACHE_TTL_SECS`].
    Cache,
    /// Cache przeterminowany, użyty po nieudanym pobraniu (albo bez sieci).
    Stale,
    /// Statyczna lista do zrzutów ekranu (`demo`).
    Demo,
    /// Nic nie ma: licznik pokaże `?`, picker powie, że brak listy.
    Missing,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Catalog {
    pub models: Vec<Model>,
    /// Kiedy listę pobrano (ms od epoch) — z cache albo z sieci.
    pub fetched_ms: u64,
    pub source: Source,
}

impl Default for Catalog {
    fn default() -> Catalog {
        Catalog {
            models: Vec::new(),
            fetched_ms: 0,
            source: Source::Missing,
        }
    }
}

impl Catalog {
    pub fn find(&self, id: &str) -> Option<&Model> {
        self.models.iter().find(|m| m.id == id)
    }

    /// Indeks aktywnego modelu w liście (0, gdy go nie ma — zaznaczenie zostaje
    /// na pierwszym wierszu, a nie poza listą).
    pub fn active_index(&self, id: &str) -> usize {
        self.models.iter().position(|m| m.id == id).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── Parsowanie odpowiedzi ───────────────────────────────────────────────────

/// Wyciąga listę modeli z odpowiedzi `/models`. Obsługiwane kształty:
/// `{"data": [...]}` (OpenAI), `{"models": [...]}` i goła tablica.
/// Wiersze bez `id` są pomijane, duplikaty `id` — tylko pierwszy.
pub fn parse_models(value: &Value) -> Vec<Model> {
    let rows = value
        .get("data")
        .and_then(|v| v.as_array())
        .or_else(|| value.get("models").and_then(|v| v.as_array()))
        .or_else(|| value.as_array());
    let mut out: Vec<Model> = Vec::new();
    for row in rows.into_iter().flatten() {
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if id.is_empty() || out.iter().any(|m| m.id == id) {
            continue;
        }
        let name = row
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&id)
            .to_string();
        out.push(Model {
            id,
            name,
            context: number(row.pointer("/limit/context")),
            output: number(row.pointer("/limit/output")),
            // Runda 15: OC-Go podaje cenę zagnieżdżoną (`cost.text.input.price`),
            // inne providery płasko (`cost.input`). Oba kształty, cena za 1M.
            price_in: decimal(row.pointer("/cost/text/input/price"))
                .or_else(|| decimal(row.pointer("/cost/input"))),
            price_out: decimal(row.pointer("/cost/text/output/price"))
                .or_else(|| decimal(row.pointer("/cost/output"))),
        });
    }
    out
}

fn number(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(n) => n
            .as_u64()
            .or_else(|| n.as_f64().map(|f| f.max(0.0).round() as u64)),
        Value::String(s) => s.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn decimal(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Wiersz cache w tym samym kształcie co odpowiedź providera, żeby
/// [`parse_models`] czytał jedno i drugie.
fn model_json(model: &Model) -> Value {
    let mut row = Map::new();
    row.insert("id".to_string(), json!(model.id));
    row.insert("name".to_string(), json!(model.name));
    if model.context.is_some() || model.output.is_some() {
        row.insert(
            "limit".to_string(),
            json!({ "context": model.context, "output": model.output }),
        );
    }
    if model.price_in.is_some() || model.price_out.is_some() {
        row.insert(
            "cost".to_string(),
            json!({ "input": model.price_in, "output": model.price_out }),
        );
    }
    Value::Object(row)
}

// ── Cache `.agent/models.json` ──────────────────────────────────────────────

pub fn cache_path(workspace: &Path) -> PathBuf {
    workspace.join(".agent").join(CACHE_FILE)
}

pub fn read_cache(workspace: &Path) -> Option<Catalog> {
    let text = fs::read_to_string(cache_path(workspace)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let models = parse_models(&value);
    if models.is_empty() {
        return None;
    }
    let fetched_ms = value.get("fetched_ms").and_then(|v| v.as_u64()).unwrap_or(0);
    Some(Catalog {
        models,
        fetched_ms,
        source: Source::Cache,
    })
}

pub fn write_cache(workspace: &Path, models: &[Model], fetched_ms: u64) -> Result<(), String> {
    let dir = workspace.join(".agent");
    fs::create_dir_all(&dir).map_err(|e| format!("nie mozna utworzyc .agent: {}", e))?;
    let rows: Vec<Value> = models.iter().map(model_json).collect();
    let body = json!({ "fetched_ms": fetched_ms, "data": rows }).to_string();
    fs::write(cache_path(workspace), body)
        .map_err(|e| format!("nie mozna zapisac models.json: {}", e))
}

/// Lista modeli na start sesji: świeży cache → sieć → przeterminowany cache →
/// pusto. Sieci nie ma w testach jednostkowych (`cfg!(test)`), żeby testy nie
/// zależały od maszyny.
pub fn load(workspace: &Path, conn: Option<&RemoteConn>) -> Catalog {
    let cached = read_cache(workspace);
    if let Some(mut fresh) = cached.clone() {
        if now_ms().saturating_sub(fresh.fetched_ms) < CACHE_TTL_SECS * 1000 {
            fresh.source = Source::Cache;
            return fresh;
        }
    }
    if let (Some(conn), false) = (conn, cfg!(test)) {
        if let Ok(value) = crate::provider::models_get(conn, FETCH_TIMEOUT_SECS) {
            let models = parse_models(&value);
            if !models.is_empty() {
                let fetched_ms = now_ms();
                let _ = write_cache(workspace, &models, fetched_ms);
                return Catalog {
                    models,
                    fetched_ms,
                    source: Source::Network,
                };
            }
        }
    }
    match cached {
        Some(mut stale) => {
            stale.source = Source::Stale;
            stale
        }
        None => Catalog::default(),
    }
}

// ── `.agent/config` (key = value) ───────────────────────────────────────────

pub fn config_path(workspace: &Path) -> PathBuf {
    workspace.join(".agent").join(CONFIG_FILE)
}

/// Wartość klucza z `.agent/config`. Format prosty: `klucz = wartość`,
/// `#` na początku to komentarz, reszta wierszy zostaje nietknięta.
pub fn config_get(workspace: &Path, key: &str) -> Option<String> {
    let text = fs::read_to_string(config_path(workspace)).ok()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Zapis klucza bez ruszania pozostałych wierszy pliku.
pub fn config_set(workspace: &Path, key: &str, value: &str) -> Result<(), String> {
    let path = config_path(workspace);
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut replaced = false;
    let mut out = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let matches = !trimmed.starts_with('#')
            && trimmed
                .split_once('=')
                .map(|(name, _)| name.trim() == key)
                .unwrap_or(false);
        if matches {
            out.push_str(&format!("{} = {}\n", key, value));
            replaced = true;
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !replaced {
        out.push_str(&format!("{} = {}\n", key, value));
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("nie mozna utworzyc .agent: {}", e))?;
    }
    fs::write(&path, out).map_err(|e| format!("nie mozna zapisac config: {}", e))
}

/// Model do użycia: `ARNDTAI_MODEL` wygrywa z plikiem `.agent/config`,
/// plik wygrywa z domyślnym modelem providera.
pub fn resolve_model(workspace: &Path, env_model: Option<&str>, default: &str) -> String {
    if let Some(model) = env_model.map(str::trim).filter(|s| !s.is_empty()) {
        return model.to_string();
    }
    config_get(workspace, "model").unwrap_or_else(|| default.to_string())
}

/// `ARNDTAI_MODEL` z otoczenia (puste traktowane jak brak).
pub fn env_model() -> Option<String> {
    env::var("ARNDTAI_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ── Etykiety ────────────────────────────────────────────────────────────────

/// `1048576` → `1.0M`, `131072` → `128K`, `950` → `950`. Progi binarne, bo
/// limity kontekstu są potęgami dwójki; jedna cyfra po kropce jak w Grok.
pub fn tokens_label(n: u64) -> String {
    if n < 1024 {
        n.to_string()
    } else if n < 1024 * 1024 {
        format!("{:.1}K", n as f64 / 1024.0)
    } else {
        format!("{:.1}M", n as f64 / (1024.0 * 1024.0))
    }
}

/// Limit kontekstu do licznika; `None` → `?` (provider nie podał, nie zgadujemy).
pub fn limit_label(context: Option<u64>) -> String {
    context.map(tokens_label).unwrap_or_else(|| "?".to_string())
}

/// Cena za 1M tokenów: `$0.05 / $0.20`; brak jednej strony to `?`, brak obu `—`.
pub fn price_label(price_in: Option<f64>, price_out: Option<f64>) -> String {
    let fmt = |v: Option<f64>| match v {
        Some(n) => format!("${:.2}", n),
        None => "?".to_string(),
    };
    match (price_in, price_out) {
        (None, None) => "\u{2014}".to_string(),
        (a, b) => format!("{} / {}", fmt(a), fmt(b)),
    }
}

// ── Lista do zrzutów (`--screenshot-model`) ─────────────────────────────────

/// Statyczna lista na zrzuty i testy układu. Bez cen — provider w tym
/// workspace nie był pytany, a zmyślona cena w pickerze byłaby nieprawdą.
pub fn demo_catalog() -> Catalog {
    let model = |id: &str, name: &str, context: u64, output: u64| Model {
        id: id.to_string(),
        name: name.to_string(),
        context: Some(context),
        output: Some(output),
        price_in: None,
        price_out: None,
    };
    Catalog {
        models: vec![
            model("mimo-v2.6-flash", "MiMo v2.6 Flash", 1_048_576, 131_072),
            model("mimo-v2.6-pro", "MiMo v2.6 Pro", 262_144, 65_536),
            model("qwen3-coder", "Qwen3 Coder", 131_072, 32_768),
            model("local-tools", "Local tools", 32_768, 8_192),
        ],
        fetched_ms: now_ms(),
        source: Source::Demo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_limit_comes_from_the_models_payload() {
        let payload = json!({
            "data": [
                {"id": "mimo-v2.6-flash", "limit": {"context": 1048576, "output": 131072},
                 "cost": {"input": 0.05, "output": 0.2},
                 "capabilities": {"tools": {"supported": true}}},
                {"id": "bez-limitu"}
            ]
        });
        let models = parse_models(&payload);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].context, Some(1_048_576));
        assert_eq!(models[0].output, Some(131_072));
        assert_eq!(limit_label(models[0].context), "1.0M");
        assert_eq!(models[0].price_in, Some(0.05));
        assert_eq!(price_label(models[0].price_in, models[0].price_out), "$0.05 / $0.20");
        // Brak `limit` to brak limitu — `?`, nie domyślne 200K.
        assert_eq!(models[1].context, None);
        assert_eq!(limit_label(models[1].context), "?");
    }

    #[test]
    fn prices_read_both_provider_shapes() {
        // Runda 15: OC-Go zwraca `cost.text.{input,output}.price`, inne
        // providery płasko `cost.{input,output}`. Oba kształty dają liczby.
        let nested = parse_models(&json!({
            "data": [{
                "id": "mimo-v2.6-flash",
                "cost": { "text": { "input": { "price": 0.05 }, "output": { "price": 0.2 } } }
            }]
        }));
        assert_eq!(nested[0].price_in, Some(0.05));
        assert_eq!(nested[0].price_out, Some(0.2));
        assert_eq!(
            price_label(nested[0].price_in, nested[0].price_out),
            "$0.05 / $0.20"
        );

        let flat = parse_models(&json!({
            "data": [{ "id": "gpt-4o", "cost": { "input": 1.25, "output": 5.0 } }]
        }));
        assert_eq!(flat[0].price_in, Some(1.25));
        assert_eq!(flat[0].price_out, Some(5.0));

        // Kształt zagnieżdżony wygrywa, gdy w odpowiedzi są oba.
        let both = parse_models(&json!({
            "data": [{
                "id": "oba",
                "cost": { "text": { "input": { "price": 3.0 } }, "input": 99.0 }
            }]
        }));
        assert_eq!(both[0].price_in, Some(3.0));

        // Brak danych (także samego `cost`, pustego `text`, ceny jako tekst)
        // daje `—` w pickerze — nigdy zera.
        let missing = parse_models(&json!({
            "data": [{ "id": "a" }, { "id": "b", "cost": {} },
                     { "id": "c", "cost": { "text": {} } }]
        }));
        for model in &missing {
            assert_eq!(model.price_in, None);
            assert_eq!(model.price_out, None);
            assert_eq!(price_label(model.price_in, model.price_out), "\u{2014}");
        }

        // Cena jako tekst (niektóre providery) też jest liczbą.
        let text = parse_models(&json!({
            "data": [{ "id": "t", "cost": { "text": { "input": { "price": "0.35" } } } }]
        }));
        assert_eq!(text[0].price_in, Some(0.35));
    }

    #[test]
    fn models_payload_may_be_a_bare_array_and_duplicates_are_dropped() {
        let models = parse_models(&json!([
            {"id": "a", "name": "A", "limit": {"context": 32768}},
            {"id": "a", "limit": {"context": 1}},
            {"id": "b"}
        ]));
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "A");
        assert_eq!(models[0].context, Some(32_768));
        assert_eq!(models[1].name, "b");
    }

    #[test]
    fn cache_round_trips_and_expires_after_an_hour() {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-models-cache-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::create_dir_all(&dir);
        let models = parse_models(&json!({"data": [{"id": "x", "limit": {"context": 1048576}}]}));
        write_cache(&dir, &models, now_ms()).unwrap();
        let cached = read_cache(&dir).expect("cache czytelny");
        assert_eq!(cached.models[0].id, "x");
        assert_eq!(cached.models[0].context, Some(1_048_576));

        let loaded = load(&dir, None);
        assert_eq!(loaded.source, Source::Cache, "świeży cache wygrywa");

        // Przeterminowany cache bez sieci: lista zostaje, źródło to Stale.
        write_cache(&dir, &models, now_ms().saturating_sub((CACHE_TTL_SECS + 5) * 1000))
            .unwrap();
        let stale = load(&dir, None);
        assert_eq!(stale.source, Source::Stale);
        assert_eq!(stale.models.len(), 1);

        // Brak cache i brak połączenia: pusto, bez błędu — licznik pokaże `?`.
        let empty_dir = dir.join("empty");
        let missing = load(&empty_dir, None);
        assert!(missing.is_empty());
        assert_eq!(missing.source, Source::Missing);
        assert_eq!(limit_label(None), "?");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_keeps_other_keys_and_env_beats_the_file() {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-models-config-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let _ = fs::create_dir_all(&dir);
        config_set(&dir, "model", "pierwszy").unwrap();
        config_set(&dir, "inne", "zostaje").unwrap();
        config_set(&dir, "model", "drugi").unwrap();
        assert_eq!(config_get(&dir, "model").as_deref(), Some("drugi"));
        assert_eq!(config_get(&dir, "inne").as_deref(), Some("zostaje"));
        // Zmienna otoczenia ma pierwszeństwo, pusta nie liczy się jako wybór.
        assert_eq!(resolve_model(&dir, Some("z-env"), "domyslny"), "z-env");
        assert_eq!(resolve_model(&dir, Some("  "), "domyslny"), "drugi");
        let no_config = dir.join("pusto");
        assert_eq!(resolve_model(&no_config, None, "domyslny"), "domyslny");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn token_labels_follow_the_grok_counter() {
        assert_eq!(tokens_label(950), "950");
        assert_eq!(tokens_label(1536), "1.5K");
        assert_eq!(tokens_label(131_072), "128.0K");
        assert_eq!(tokens_label(1_048_576), "1.0M");
        assert_eq!(price_label(None, None), "\u{2014}");
        assert_eq!(price_label(Some(0.5), None), "$0.50 / ?");
    }

    #[test]
    fn demo_catalog_has_the_models_the_picker_shows() {
        let catalog = demo_catalog();
        assert_eq!(catalog.source, Source::Demo);
        assert_eq!(catalog.active_index("mimo-v2.6-pro"), 1);
        assert_eq!(catalog.active_index("nie-ma"), 0);
        assert_eq!(
            limit_label(catalog.find("mimo-v2.6-flash").unwrap().context),
            "1.0M"
        );
    }
}
