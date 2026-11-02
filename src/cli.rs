use crate::island::Page;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliAction {
    Run,
    Toggle,
    OpenPage(Page),
    Settings,
    Terminal,
}

const PAGE_NAMES: [(&str, Page); 6] = [
    ("musica", Page::Musica),
    ("calendario", Page::Calendario),
    ("todo", Page::Todo),
    ("shelf", Page::Shelf),
    ("escritorios", Page::Escritorios),
    ("dev", Page::Dev),
];

/// Nombres en inglés aceptados además de los españoles (`todo`, `shelf` y `dev` coinciden).
const PAGE_ALIASES: [(&str, Page); 6] = [
    ("music", Page::Musica),
    ("calendar", Page::Calendario),
    ("todo", Page::Todo),
    ("shelf", Page::Shelf),
    ("desktops", Page::Escritorios),
    ("dev", Page::Dev),
];

fn page_from_name(name: &str) -> Option<Page> {
    let name = name.to_lowercase();
    let name = if name == "música" {
        "musica"
    } else {
        name.as_str()
    };
    if let Some((_, p)) = PAGE_ALIASES.iter().find(|(n, _)| *n == name) {
        return Some(*p);
    }
    PAGE_NAMES.iter().find(|(n, _)| *n == name).map(|(_, p)| *p)
}

fn page_name(page: Page) -> &'static str {
    PAGE_NAMES
        .iter()
        .find(|(_, p)| *p == page)
        .map(|(n, _)| *n)
        .unwrap_or("musica")
}

fn valid_pages() -> String {
    PAGE_NAMES
        .iter()
        .map(|(n, p)| match PAGE_ALIASES.iter().find(|(_, q)| q == p) {
            Some((a, _)) if a != n => format!("{n} ({a})"),
            _ => (*n).to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Interpreta los argumentos (sin el nombre del programa).
pub fn parse(args: &[String]) -> Result<CliAction, String> {
    let mut action = CliAction::Run;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--toggle" => action = CliAction::Toggle,
            "--settings" => action = CliAction::Settings,
            "--terminal" => action = CliAction::Terminal,
            "--page" => {
                let name = it
                    .next()
                    .ok_or_else(|| crate::fl!("cli-page-needs-name", pages = valid_pages()))?;
                action = CliAction::OpenPage(page_from_name(name).ok_or_else(|| {
                    crate::fl!(
                        "cli-unknown-page",
                        name = name.as_str(),
                        pages = valid_pages()
                    )
                })?);
            }
            other => {
                return Err(crate::fl!("cli-unknown-arg", arg = other));
            }
        }
    }
    Ok(action)
}

impl CliAction {
    /// Cadena enviada a la instancia en ejecución por activación D-Bus.
    pub fn to_action_string(self) -> Option<String> {
        match self {
            CliAction::Run => None,
            CliAction::Toggle => Some("toggle".into()),
            CliAction::OpenPage(p) => Some(format!("page:{}", page_name(p))),
            CliAction::Settings => Some("settings".into()),
            CliAction::Terminal => Some("terminal".into()),
        }
    }

    pub fn from_action_string(s: &str) -> Option<CliAction> {
        match s {
            "toggle" => Some(CliAction::Toggle),
            "settings" => Some(CliAction::Settings),
            "terminal" => Some(CliAction::Terminal),
            _ => s
                .strip_prefix("page:")
                .and_then(page_from_name)
                .map(CliAction::OpenPage),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_basic() {
        assert_eq!(parse(&[]), Ok(CliAction::Run));
        assert_eq!(parse(&v(&["--toggle"])), Ok(CliAction::Toggle));
        assert_eq!(parse(&v(&["--settings"])), Ok(CliAction::Settings));
        assert_eq!(parse(&v(&["--terminal"])), Ok(CliAction::Terminal));
        assert_eq!(
            parse(&v(&["--page", "escritorios"])),
            Ok(CliAction::OpenPage(Page::Escritorios))
        );
        assert_eq!(
            parse(&v(&["--page", "música"])),
            Ok(CliAction::OpenPage(Page::Musica))
        );
    }

    #[test]
    fn parse_english_page_names() {
        for (name, page) in [
            ("music", Page::Musica),
            ("calendar", Page::Calendario),
            ("todo", Page::Todo),
            ("shelf", Page::Shelf),
            ("desktops", Page::Escritorios),
            ("dev", Page::Dev),
            ("Music", Page::Musica),
        ] {
            assert_eq!(
                parse(&v(&["--page", name])),
                Ok(CliAction::OpenPage(page)),
                "{name}"
            );
        }
        // La cadena D-Bus sigue usando el nombre español.
        assert_eq!(
            CliAction::OpenPage(Page::Musica)
                .to_action_string()
                .as_deref(),
            Some("page:musica")
        );
        assert_eq!(
            CliAction::from_action_string("page:calendar"),
            Some(CliAction::OpenPage(Page::Calendario))
        );
        crate::i18n::apply_for_test("en");
        let e = parse(&v(&["--page", "x"])).unwrap_err();
        assert!(e.contains("musica (music)") && e.contains("todo"), "{e}");
    }

    #[test]
    fn parse_errors_in_both_languages() {
        crate::i18n::apply_for_test("es");
        assert!(
            parse(&v(&["--wat"]))
                .unwrap_err()
                .starts_with("argumento desconocido '--wat'")
        );
        assert!(
            parse(&v(&["--page", "x"]))
                .unwrap_err()
                .starts_with("página desconocida 'x'")
        );
        crate::i18n::apply_for_test("en");
        assert!(
            parse(&v(&["--wat"]))
                .unwrap_err()
                .starts_with("unknown argument '--wat'")
        );
        assert!(
            parse(&v(&["--page", "x"]))
                .unwrap_err()
                .starts_with("unknown page 'x'")
        );
        assert!(
            parse(&v(&["--page"]))
                .unwrap_err()
                .starts_with("--page requires a name")
        );
    }

    #[test]
    fn parse_errors_list_pages() {
        let e = parse(&v(&["--page", "nada"])).unwrap_err();
        for n in [
            "musica",
            "calendario",
            "todo",
            "shelf",
            "escritorios",
            "dev",
        ] {
            assert!(e.contains(n), "{e}");
        }
        assert!(parse(&v(&["--page"])).is_err());
        assert!(parse(&v(&["--wat"])).is_err());
    }

    #[test]
    fn action_string_roundtrip() {
        for a in [
            CliAction::Toggle,
            CliAction::Settings,
            CliAction::Terminal,
            CliAction::OpenPage(Page::Dev),
            CliAction::OpenPage(Page::Musica),
        ] {
            let s = a.to_action_string().unwrap();
            assert_eq!(CliAction::from_action_string(&s), Some(a));
        }
        assert_eq!(CliAction::Run.to_action_string(), None);
        assert_eq!(CliAction::from_action_string("page:x"), None);
    }
}
