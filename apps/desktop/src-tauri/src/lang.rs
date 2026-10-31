//! Langue des libellés : anglais, ou français si Windows l'est (`SOLON_LANG=fr` force).

use std::sync::OnceLock;

static FRENCH: OnceLock<bool> = OnceLock::new();

pub fn french() -> bool {
    *FRENCH.get_or_init(|| {
        if let Ok(v) = std::env::var("SOLON_LANG")
            && !v.is_empty()
        {
            return v.to_lowercase().starts_with("fr");
        }
        // SAFETY : appel sans argument ni pointeur.
        let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        (id & 0x3ff) == 0x0c // LANG_FRENCH
    })
}

pub fn tr<'a>(en: &'a str, fr: &'a str) -> &'a str {
    if french() { fr } else { en }
}
