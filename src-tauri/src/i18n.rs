//! The handful of strings Rust itself shows: system notifications and the tray
//! menu. Everything else lives in `src/i18n/locales` on the frontend.
//!
//! Deliberately a plain lookup table rather than a full i18n crate — there is a
//! handful of strings, and they must stay in sync with the JSON bundles by hand.

pub fn tray_open(lang: &str) -> &'static str {
    match lang {
        "es" => "Abrir NihongoTracker Companion",
        _ => "Open NihongoTracker Companion",
    }
}

pub fn tray_sync(lang: &str) -> &'static str {
    match lang {
        "es" => "Sincronizar ahora",
        _ => "Sync now",
    }
}

pub fn tray_quit(lang: &str) -> &'static str {
    match lang {
        "es" => "Salir",
        _ => "Quit",
    }
}

pub fn episode_logged(lang: &str) -> &'static str {
    match lang {
        "es" => "Episodio registrado",
        _ => "Episode logged",
    }
}

pub fn ready_to_log(lang: &str) -> &'static str {
    match lang {
        "es" => "Listo para registrar",
        _ => "Ready to log",
    }
}

pub fn session_started(lang: &str) -> &'static str {
    match lang {
        "es" => "Sesión iniciada",
        _ => "Session started",
    }
}

pub fn session_ended(lang: &str) -> &'static str {
    match lang {
        "es" => "Sesión terminada",
        _ => "Session finished",
    }
}

pub fn session_logged(lang: &str) -> &'static str {
    match lang {
        "es" => "Sesión registrada",
        _ => "Session logged",
    }
}

pub fn detected(lang: &str, title: &str) -> String {
    match lang {
        "es" => format!("{title} detectada"),
        _ => format!("{title} detected"),
    }
}

pub fn episode_label(lang: &str, episode: Option<u32>) -> String {
    match (lang, episode) {
        ("es", Some(n)) => format!("ep. {n}"),
        ("es", None) => "episodio".into(),
        (_, Some(n)) => format!("ep. {n}"),
        (_, None) => "episode".into(),
    }
}

pub fn streak_title(lang: &str) -> &'static str {
    match lang {
        "es" => "Racha en riesgo",
        _ => "Streak at risk",
    }
}

pub fn streak_action(lang: &str) -> &'static str {
    match lang {
        "es" => "Registrar ahora",
        _ => "Log now",
    }
}

pub fn streak_body(lang: &str, streak: i64, minutes_left: i64) -> String {
    let left = time_left(lang, minutes_left);
    match lang {
        "es" => format!("{streak} días en juego y hoy no has registrado nada. Quedan {left}."),
        _ => format!("{streak}-day streak, nothing logged today. {left} left."),
    }
}

fn time_left(lang: &str, minutes: i64) -> String {
    let minutes = minutes.max(0);
    let (hours, rest) = (minutes / 60, minutes % 60);
    match (lang, hours) {
        ("es", 0) => format!("{rest} min"),
        ("es", _) => format!("{hours} h {rest} min"),
        (_, 0) => format!("{rest} min"),
        (_, _) => format!("{hours} h {rest} min"),
    }
}

// ── Discord Rich Presence ──────────────────────────────────────────────
//
// These go out to other people's screens, not to the user's, so they follow the
// user's own language: it is their profile.

pub fn discord_generic(lang: &str) -> &'static str {
    match lang {
        "es" => "Inmersión en japonés",
        _ => "Japanese immersion",
    }
}

pub fn discord_profile(lang: &str) -> &'static str {
    match lang {
        "es" => "Ver perfil",
        _ => "View profile",
    }
}

pub fn discord_reading(lang: &str, title: &str) -> String {
    match lang {
        "es" => format!("Leyendo {title}"),
        _ => format!("Reading {title}"),
    }
}

pub fn discord_watching(lang: &str, title: &str) -> String {
    match lang {
        "es" => format!("Viendo {title}"),
        _ => format!("Watching {title}"),
    }
}

pub fn discord_episode_state(lang: &str, episode: Option<u32>) -> String {
    match (lang, episode) {
        ("es", Some(n)) => format!("Episodio {n}"),
        ("es", None) => "En curso".into(),
        (_, Some(n)) => format!("Episode {n}"),
        (_, None) => "In progress".into(),
    }
}

pub fn discord_chars(lang: &str, chars: i64) -> String {
    let count = compact_chars(lang, chars);
    match lang {
        "es" => format!("{count} caracteres"),
        _ => format!("{count} characters"),
    }
}

/// `12.4k`, with the decimal separator of the language.
fn compact_chars(lang: &str, chars: i64) -> String {
    if chars < 1000 {
        return chars.to_string();
    }
    let thousands = chars as f64 / 1000.0;
    let formatted = format!("{thousands:.1}k");
    match lang {
        "es" => formatted.replace('.', ","),
        _ => formatted,
    }
}

pub fn session_summary(lang: &str, minutes: u64, chars: i64) -> String {
    match lang {
        "es" => format!("{minutes} min, {chars} car."),
        _ => format!("{minutes} min, {chars} chars"),
    }
}
