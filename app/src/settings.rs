//! Ustawienia TUI (runda 15): sześć kluczy w `.agent/config`, walidacja
//! i cykliczna zmiana wartości.
//!
//! Plik `.agent/config` ma format `klucz = wartość` (ten sam, w którym picker
//! `/model` zapisuje `model`). Tutaj dochodzi sześć kluczy widocznych
//! w modalu `/settings`:
//!
//! | klucz | wartości | skutek |
//! |---|---|---|
//! | `theme` | `groknight`, `jasny` | paleta TUI |
//! | `approval` | `always-approve`, `ask` | pytanie przed efektem ubocznym |
//! | `memory` | `lite`, `pro`, `ultra` | jak `--memory` |
//! | `timestamps` | `on`, `off` | godzina przy wpisie użytkownika |
//! | `mouse` | `on`, `off` | przechwytywanie kółka i kliknięć |
//! | `max_tokens` | `1024`, `2048`, `4096`, `8192` | limit odpowiedzi modelu |
//!
//! Precedencja: zmienne środowiskowe (`ARNDTAI_MODEL`, `ARNDTAI_TIMEOUT`) →
//! `--memory` z argv → `.agent/config` → wartość domyślna. Nieznana wartość
//! w pliku **nie** zatrzymuje programu: zostaje wartość domyślna, a w
//! scrollbacku pojawia się ostrzeżenie z nazwą klucza.

use crate::context::MemoryLevel;
use crate::models::{config_get, config_set};
use std::env;
use std::path::Path;

/// Klucze w kolejności, w jakiej stoją w modalu.
pub const KEYS: [&str; 6] = ["theme", "approval", "memory", "timestamps", "mouse", "max_tokens"];

/// Dozwolone limity odpowiedzi (`max_tokens`).
pub const TOKEN_LIMITS: [usize; 4] = [1024, 2048, 4096, 8192];

pub const MAX_TOKENS_DEFAULT: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Groknight,
    Light,
}

impl Theme {
    pub fn parse(text: &str) -> Option<Theme> {
        match text.trim().to_ascii_lowercase().as_str() {
            "groknight" => Some(Theme::Groknight),
            "jasny" => Some(Theme::Light),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Theme::Groknight => "groknight",
            Theme::Light => "jasny",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Approval {
    AlwaysApprove,
    Ask,
}

impl Approval {
    pub fn parse(text: &str) -> Option<Approval> {
        match text.trim().to_ascii_lowercase().as_str() {
            "always-approve" => Some(Approval::AlwaysApprove),
            "ask" => Some(Approval::Ask),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Approval::AlwaysApprove => "always-approve",
            Approval::Ask => "ask",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Theme,
    pub approval: Approval,
    pub memory: MemoryLevel,
    /// Godzina przy wpisie użytkownika (pas zadania).
    pub timestamps: bool,
    /// Przechwytywanie myszy (`EnableMouseCapture`).
    pub mouse: bool,
    /// Sufit `max_tokens` żądania; nigdy więcej niż `limit.output` modelu.
    pub max_tokens: usize,
    /// Ostrzeżenia z walidacji pliku — pokazywane w scrollbacku.
    pub warnings: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            theme: Theme::Groknight,
            approval: Approval::AlwaysApprove,
            memory: MemoryLevel::Lite,
            timestamps: true,
            mouse: true,
            max_tokens: MAX_TOKENS_DEFAULT,
            warnings: Vec::new(),
        }
    }
}

impl Settings {
    /// Odczyt wszystkich sześciu kluczy z `.agent/config`. Brak klucza to
    /// wartość domyślna bez ostrzeżenia; nieznana wartość — domyślna
    /// z ostrzeżeniem.
    pub fn load(workspace: &Path) -> Settings {
        let mut settings = Settings::default();
        let mut warnings: Vec<String> = Vec::new();

        if let Some(raw) = config_get(workspace, "theme") {
            match Theme::parse(&raw) {
                Some(theme) => settings.theme = theme,
                None => warnings.push(unknown("theme", &raw, settings.theme.as_str())),
            }
        }
        if let Some(raw) = config_get(workspace, "approval") {
            match Approval::parse(&raw) {
                Some(approval) => settings.approval = approval,
                None => warnings.push(unknown("approval", &raw, settings.approval.as_str())),
            }
        }
        if let Some(raw) = config_get(workspace, "memory") {
            match MemoryLevel::parse(&raw) {
                Some(level) => settings.memory = level,
                None => warnings.push(unknown("memory", &raw, settings.memory.as_str())),
            }
        }
        if let Some(raw) = config_get(workspace, "timestamps") {
            match toggle(&raw) {
                Some(on) => settings.timestamps = on,
                None => warnings.push(unknown("timestamps", &raw, "on")),
            }
        }
        if let Some(raw) = config_get(workspace, "mouse") {
            match toggle(&raw) {
                Some(on) => settings.mouse = on,
                None => warnings.push(unknown("mouse", &raw, "on")),
            }
        }
        if let Some(raw) = config_get(workspace, "max_tokens") {
            match raw.trim().parse::<usize>() {
                Ok(n) if TOKEN_LIMITS.contains(&n) => settings.max_tokens = n,
                _ => warnings.push(unknown(
                    "max_tokens",
                    &raw,
                    &settings.max_tokens.to_string(),
                )),
            }
        }

        settings.warnings = warnings;
        settings
    }

    /// Zapis wszystkich sześciu kluczy. `config_set` podmienia tylko wskazany
    /// wiersz, więc `model` i komentarze w pliku zostają.
    pub fn save(&self, workspace: &Path) -> Result<(), String> {
        for (key, value) in self.pairs() {
            config_set(workspace, key, &value)?;
        }
        Ok(())
    }

    /// Pary `(klucz, wartość)` w kolejności modala — do zapisu i do testów.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        (0..KEYS.len()).map(|i| (KEYS[i], self.value(i))).collect()
    }

    /// Etykieta wiersza w modalu (kolejność jak [`KEYS`]).
    pub fn label(index: usize) -> &'static str {
        match index {
            0 => "motyw",
            1 => "tryb zgod",
            2 => "poziom pamieci",
            3 => "znaczniki czasu",
            4 => "raportowanie myszy",
            _ => "limit odpowiedzi",
        }
    }

    /// Wartość wiersza `index` w postaci pokazywanej w modalu.
    pub fn value(&self, index: usize) -> String {
        match index {
            0 => self.theme.as_str().to_string(),
            1 => self.approval.as_str().to_string(),
            2 => self.memory.as_str().to_string(),
            3 => on_off(self.timestamps).to_string(),
            4 => on_off(self.mouse).to_string(),
            _ => self.max_tokens.to_string(),
        }
    }

    /// `←`/`→`: poprzednia/następna wartość (cyklicznie). Zwraca `true`, gdy
    /// wartość naprawdę się zmieniła.
    pub fn cycle(&mut self, index: usize, dir: i32) -> bool {
        let before = self.value(index);
        let next = dir >= 0;
        match index {
            0 => {
                self.theme = match self.theme {
                    Theme::Groknight => Theme::Light,
                    Theme::Light => Theme::Groknight,
                }
            }
            1 => {
                self.approval = match self.approval {
                    Approval::AlwaysApprove => Approval::Ask,
                    Approval::Ask => Approval::AlwaysApprove,
                }
            }
            2 => {
                self.memory = match (self.memory.clone(), next) {
                    (MemoryLevel::Lite, true) => MemoryLevel::Pro,
                    (MemoryLevel::Lite, false) => MemoryLevel::Ultra,
                    (MemoryLevel::Pro, true) => MemoryLevel::Ultra,
                    (MemoryLevel::Pro, false) => MemoryLevel::Lite,
                    (MemoryLevel::Ultra, true) => MemoryLevel::Lite,
                    (MemoryLevel::Ultra, false) => MemoryLevel::Pro,
                }
            }
            3 => self.timestamps = !self.timestamps,
            4 => self.mouse = !self.mouse,
            _ => {
                let pos = TOKEN_LIMITS
                    .iter()
                    .position(|n| *n == self.max_tokens)
                    .unwrap_or(TOKEN_LIMITS.len() - 1);
                let len = TOKEN_LIMITS.len();
                let step = if next { pos + 1 } else { pos + len - 1 };
                self.max_tokens = TOKEN_LIMITS[step % len];
            }
        }
        self.value(index) != before
    }

    /// Sufit `max_tokens` dla modelu: mniejsza z dwóch wartości — ustawienie
    /// i `limit.output` providera. Brak metadanych = samo ustawienie.
    pub fn token_budget(&self, model_limit: Option<u64>) -> usize {
        match model_limit {
            Some(limit) => self.max_tokens.min(limit as usize),
            None => self.max_tokens,
        }
    }
}

/// Wiersze tylko do odczytu: zmienne środowiskowe mają pierwszeństwo przed
/// plikiem, więc modal pokazuje je jako „z env".
pub fn env_rows() -> Vec<(String, String)> {
    let mut rows = Vec::new();
    for key in ["ARNDTAI_MODEL", "ARNDTAI_TIMEOUT"] {
        if let Ok(value) = env::var(key) {
            let value = value.trim().to_string();
            if !value.is_empty() {
                rows.push((key.to_string(), value));
            }
        }
    }
    rows
}

fn unknown(key: &str, raw: &str, fallback: &str) -> String {
    format!(
        "config: {} = '{}' nieznane - zostaje {}",
        key,
        raw.trim(),
        fallback
    )
}

fn toggle(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_ws(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arndtai-settings-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn all_six_keys_round_trip_through_the_file() {
        let ws = temp_ws("roundtrip");
        let settings = Settings {
            theme: Theme::Light,
            approval: Approval::Ask,
            memory: MemoryLevel::Ultra,
            timestamps: false,
            mouse: false,
            max_tokens: 2048,
            warnings: Vec::new(),
        };
        settings.save(&ws).expect("zapis configu");

        let text = fs::read_to_string(ws.join(".agent").join("config")).unwrap();
        for key in KEYS {
            assert!(text.contains(key), "brak klucza {} w pliku:\n{}", key, text);
        }

        let back = Settings::load(&ws);
        assert_eq!(back.theme, Theme::Light);
        assert_eq!(back.approval, Approval::Ask);
        assert_eq!(back.memory, MemoryLevel::Ultra);
        assert!(!back.timestamps);
        assert!(!back.mouse);
        assert_eq!(back.max_tokens, 2048);
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);

        // Domyślne ustawienia wracają po zapisie domyślnych wartości.
        Settings::default().save(&ws).unwrap();
        let defaults = Settings::load(&ws);
        assert_eq!(defaults.theme, Theme::Groknight);
        assert_eq!(defaults.approval, Approval::AlwaysApprove);
        assert_eq!(defaults.memory, MemoryLevel::Lite);
        assert!(defaults.timestamps && defaults.mouse);
        assert_eq!(defaults.max_tokens, 4096);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn bad_values_fall_back_to_defaults_with_a_warning() {
        let ws = temp_ws("bad");
        fs::create_dir_all(ws.join(".agent")).unwrap();
        fs::write(
            ws.join(".agent").join("config"),
            "theme = neon\napproval = zawsze\nmemory = max\n\
             timestamps = tak\nmouse = nie\nmax_tokens = 123\nmodel = zostaje\n",
        )
        .unwrap();

        let settings = Settings::load(&ws);
        assert_eq!(settings.theme, Theme::Groknight);
        assert_eq!(settings.approval, Approval::AlwaysApprove);
        assert_eq!(settings.memory, MemoryLevel::Lite);
        assert!(settings.timestamps, "domyslnie wlaczone");
        assert!(settings.mouse, "domyslnie wlaczone");
        assert_eq!(settings.max_tokens, 4096);
        assert_eq!(settings.warnings.len(), 6, "{:?}", settings.warnings);
        assert!(
            settings.warnings.iter().any(|w| w.contains("theme")),
            "ostrzezenie musi nazywac klucz: {:?}",
            settings.warnings
        );

        // Zapis po ostrzezeniach nie rusza klucza `model` (picker `/model`).
        settings.save(&ws).unwrap();
        assert_eq!(config_get(&ws, "model").as_deref(), Some("zostaje"));
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn missing_file_gives_defaults_without_warnings() {
        let ws = temp_ws("empty");
        let settings = Settings::load(&ws.join("brak"));
        assert_eq!(settings, Settings::default());
        assert!(settings.warnings.is_empty());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn cycle_walks_the_allowed_values_and_wraps() {
        let mut settings = Settings::default();
        // Motyw i tryb zgod: dwie wartości, `→` w kółko.
        assert!(settings.cycle(0, 1));
        assert_eq!(settings.value(0), "jasny");
        assert!(settings.cycle(0, 1));
        assert_eq!(settings.value(0), "groknight");
        assert!(settings.cycle(1, -1));
        assert_eq!(settings.value(1), "ask");
        // Pamięć: trzy poziomy.
        settings.memory = MemoryLevel::Lite;
        assert!(settings.cycle(2, 1));
        assert_eq!(settings.value(2), "pro");
        assert!(settings.cycle(2, 1));
        assert_eq!(settings.value(2), "ultra");
        assert!(settings.cycle(2, 1));
        assert_eq!(settings.value(2), "lite");
        // Limit odpowiedzi: cztery wartości, zawija się.
        settings.max_tokens = 8192;
        assert!(settings.cycle(5, 1));
        assert_eq!(settings.value(5), "1024");
        assert!(settings.cycle(5, -1));
        assert_eq!(settings.value(5), "8192");
        assert!(settings.cycle(5, -1));
        assert_eq!(settings.value(5), "4096");
        // Znaczniki czasu i mysz: przełączniki.
        assert!(settings.cycle(3, 1));
        assert_eq!(settings.value(3), "off");
        assert!(settings.cycle(4, 1));
        assert_eq!(settings.value(4), "off");
    }

    #[test]
    fn token_budget_never_exceeds_the_model_limit() {
        let mut settings = Settings::default();
        assert_eq!(settings.token_budget(None), 4096);
        assert_eq!(settings.token_budget(Some(1000)), 1000);
        assert_eq!(settings.token_budget(Some(131_072)), 4096);
        settings.max_tokens = 1024;
        assert_eq!(settings.token_budget(Some(131_072)), 1024);
    }
}
