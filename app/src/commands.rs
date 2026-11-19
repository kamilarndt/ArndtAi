//! Rejestr komend `/` (runda 14).
//!
//! Jedno miejsce na nazwę, opis, „czy wymaga argumentu" i handler — TUI nie
//! ma własnej listy komend obok tej. Na start sześć pozycji; kolejne rundy
//! (settings, skills) dopisują się tutaj, a nie w `tui.rs`.

/// Co komenda robi. TUI tłumaczy to na akcję; rejestr nie zna ekranu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Handler {
    /// Tabela komend w scrollbacku.
    Help,
    /// Modal pickera modeli.
    Model,
    /// Modal ustawień (runda 15).
    Settings,
    /// `/theme jasny|groknight` — motyw od razu (runda 15).
    Theme,
    /// Czyszczenie scrollbacku.
    Clear,
    /// Modal „Resume session".
    Resume,
    /// Modal listy skilli (runda 16).
    Skills,
    /// `/skill <nazwa>` — treść skilla do kontekstu (runda 16).
    Skill,
    /// `/copy [all]` — schowek przez OSC 52 (runda 17).
    Copy,
    /// `/export [plik]` — transkrypt rozmowy do `.md` (runda 17).
    Export,
    /// Wyjście z programu.
    Quit,
}

pub struct Command {
    /// Nazwa z ukośnikiem, dokładnie tak, jak się ją wpisuje.
    pub name: &'static str,
    pub about: &'static str,
    /// `true`, gdy komenda bez argumentu nie ma sensu (żadna z startowych).
    pub needs_arg: bool,
    pub handler: Handler,
}

const COMMANDS: [Command; 11] = [
    Command {
        name: "/help",
        about: "Pokaz dostepne komendy",
        needs_arg: false,
        handler: Handler::Help,
    },
    Command {
        name: "/model",
        about: "Wybierz model z listy providera",
        needs_arg: false,
        handler: Handler::Model,
    },
    Command {
        name: "/settings",
        about: "Ustawienia: motyw, zgody, pamiec, limit",
        needs_arg: false,
        handler: Handler::Settings,
    },
    Command {
        name: "/theme",
        about: "Motyw od razu: /theme jasny | groknight",
        needs_arg: true,
        handler: Handler::Theme,
    },
    Command {
        name: "/clear",
        about: "Wyczysc scrollback",
        needs_arg: false,
        handler: Handler::Clear,
    },
    Command {
        name: "/resume",
        about: "Otworz modal „Resume session\"",
        needs_arg: false,
        handler: Handler::Resume,
    },
    Command {
        name: "/skills",
        about: "Lista skilli: nazwa, opis, rozmiar",
        needs_arg: false,
        handler: Handler::Skills,
    },
    Command {
        name: "/skill",
        about: "Wczytaj tresc skilla do kontekstu: /skill <nazwa>",
        needs_arg: true,
        handler: Handler::Skill,
    },
    Command {
        name: "/copy",
        about: "Kopiuj odpowiedz (OSC 52): /copy all — cala rozmowe",
        needs_arg: false,
        handler: Handler::Copy,
    },
    Command {
        name: "/export",
        about: "Eksport rozmowy do .md: /export [plik]",
        needs_arg: false,
        handler: Handler::Export,
    },
    Command {
        name: "/quit",
        about: "Zakoncz program",
        needs_arg: false,
        handler: Handler::Quit,
    },
];

pub fn all() -> &'static [Command] {
    &COMMANDS
}

/// Komenda po pełnej nazwie; argumenty po spacji są ignorowane.
pub fn exact(text: &str) -> Option<&'static Command> {
    let name = text.split_whitespace().next().unwrap_or("");
    COMMANDS.iter().find(|c| c.name == name)
}

/// Podpowiedzi po prefiksie („/mo" → `/model`). Pusty prefiks po ukośniku
/// zwraca wszystkie komendy. Tekst z odstępem to już argument — brak podpowiedzi.
pub fn matches(prefix: &str) -> Vec<&'static Command> {
    let text = prefix.trim_start();
    if !text.starts_with('/') || text.contains(char::is_whitespace) {
        return Vec::new();
    }
    COMMANDS
        .iter()
        .filter(|c| c.name.starts_with(text))
        .collect()
}

/// Tabela do `/help`: wyrównane kolumny `nazwa` + opis (format z Groka:
/// dwie spacje wcięcia, nazwa dopełniona do najdłuższej, dwa odstępy).
pub fn help_text() -> String {
    let width = all()
        .iter()
        .map(|c| c.name.chars().count())
        .max()
        .unwrap_or(0);
    let mut out = String::from("Komendy:");
    for command in all().iter() {
        let mark = if command.needs_arg { " *" } else { "" };
        out.push_str(&format!(
            "\n  {:<width$}  {}{}",
            command.name,
            command.about,
            mark,
            width = width
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eleven_commands_are_registered_once() {
        assert_eq!(all().len(), 11);
        let names: Vec<&str> = all().iter().map(|c| c.name).collect();
        assert_eq!(
            names,
            vec![
                "/help",
                "/model",
                "/settings",
                "/theme",
                "/clear",
                "/resume",
                "/skills",
                "/skill",
                "/copy",
                "/export",
                "/quit"
            ]
        );
        // Tylko `/theme` ma sens wyłącznie z argumentem (nazwa motywu).
        let with_arg: Vec<&str> = all()
            .iter()
            .filter(|c| c.needs_arg)
            .map(|c| c.name)
            .collect();
        assert_eq!(with_arg, vec!["/theme", "/skill"]);
    }

    #[test]
    fn prefix_filter_returns_the_matching_command() {
        assert_eq!(matches("/mo").len(), 1);
        assert_eq!(matches("/mo")[0].name, "/model");
        assert_eq!(matches("/t").len(), 1);
        assert_eq!(matches("/sk").len(), 2, "/skills i /skill");
        assert_eq!(matches("/skills").len(), 1, "dokladna nazwa to jedna komenda");
        assert_eq!(matches("/t")[0].name, "/theme");
        assert_eq!(matches("/").len(), 11, "sam ukośnik pokazuje wszystkie");
        assert!(matches("/model cos").is_empty(), "argument to nie prefiks");
        assert!(matches("model").is_empty(), "bez ukośnika nie ma komend");
        assert!(matches("/nie-ma").is_empty());
        assert!(exact("/model").is_some());
        assert!(exact("/model arg").is_some());
        assert!(exact("/mo").is_none());
        assert!(exact("/nieznana").is_none());
    }

    #[test]
    fn help_table_is_aligned_and_has_all_rows() {
        let text = help_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 12, "naglowek + 11 komend: {text}");
        assert!(lines[0].starts_with("Komendy:"));
        // Opisy zaczynają się w tej samej kolumnie w każdym wierszu.
        let columns: Vec<usize> = all()
            .iter()
            .map(|command| {
                lines[1..]
                    .iter()
                    .find_map(|line| line.find(command.about))
                    .unwrap_or(0)
            })
            .collect();
        assert!(
            columns.windows(2).all(|w| w[0] == w[1]),
            "kolumna opisu stala: {:?}",
            columns
        );
        for command in all() {
            assert!(text.contains(command.name), "brak {} w tabeli", command.name);
        }
    }
}
