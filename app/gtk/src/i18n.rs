use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain, LocaleCategory};

use crate::config::{GETTEXT_PACKAGE, LOCALE_DIR};

pub fn init(language: Option<&str>) {
    match language {
        Some("pt_BR") => unsafe {
            std::env::set_var("LANGUAGE", "pt_BR");
            std::env::set_var("LANG", "pt_BR.UTF-8");
            std::env::remove_var("LC_ALL");
        },
        Some("en") => unsafe {
            std::env::set_var("LANGUAGE", "en");
            std::env::set_var("LANG", "en_US.UTF-8");
            std::env::remove_var("LC_ALL");
        },
        _ => {}
    }

    gettextrs::setlocale(LocaleCategory::LcAll, "");

    let _ = gettextrs::setlocale(LocaleCategory::LcMessages, "");

    if let Err(e) = bindtextdomain(GETTEXT_PACKAGE, LOCALE_DIR) {
        log::warn!("Could not bind gettext domain to {}: {e}", LOCALE_DIR);
        return;
    }
    if let Err(e) = bind_textdomain_codeset(GETTEXT_PACKAGE, "UTF-8") {
        log::warn!("Could not set gettext codeset: {e}");
        return;
    }
    if let Err(e) = textdomain(GETTEXT_PACKAGE) {
        log::warn!("Could not activate gettext domain: {e}");
        return;
    }

    log::info!(
        "i18n initialized: requested={:?} LANGUAGE={:?} LANG={:?} LOCALE_DIR={} sample_receive='{}' sample_visibility='{}'",
        language,
        std::env::var("LANGUAGE").ok(),
        std::env::var("LANG").ok(),
        LOCALE_DIR,
        gettextrs::dgettext(GETTEXT_PACKAGE, "Receive"),
        gettextrs::dgettext(GETTEXT_PACKAGE, "Visibility"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_pt_br_translations() {
        init(Some("pt_BR"));
        assert_eq!(gettextrs::dgettext(GETTEXT_PACKAGE, "Receive"), "Receber");
        assert_eq!(gettextrs::dgettext(GETTEXT_PACKAGE, "Visibility"), "Visibilidade");
    }
}

#[macro_export]
macro_rules! tr {
    ($s:literal) => {
        gettextrs::dgettext(crate::config::GETTEXT_PACKAGE, $s)
    };
    ($s:literal, $($arg:tt)*) => {
        format!(gettextrs::dgettext(crate::config::GETTEXT_PACKAGE, $s), $($arg)*)
    };
}
