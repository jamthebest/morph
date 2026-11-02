mod agenda;
mod anim;
mod app;
mod cli;
mod clock;
mod config;
mod desktops;
mod dev;
mod dnd;
mod drag;
mod envcompat;
mod geometry;
mod http;
mod hud;
mod i18n;
mod island;
mod lyrics;
mod media;
mod migrate;
mod music;
mod notice;
mod peek;
mod reveal;
mod scroll;
mod settings;
mod shelf;
mod sound;
mod store_util;
mod sys;
mod term;
mod volume;

fn main() -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default()
        .no_main_window(true)
        .exit_on_close(false);
    // Mensajes de la CLI: idioma de la config (o del escritorio si no se puede leer).
    let language = {
        use cosmic::cosmic_config::CosmicConfigEntry;
        cosmic::cosmic_config::Config::new(app::APP_ID, 1)
            .map(|c| config::MorphConfig::get_entry(&c).unwrap_or_else(|(_errs, cfg)| cfg))
            .unwrap_or_default()
            .language
    };
    i18n::apply(language);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = match cli::parse(&args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("morph: {e}");
            std::process::exit(2);
        }
    };
    migrate::run();
    cosmic::app::run_single_instance::<app::App>(settings, app::Flags::new(action))
}
