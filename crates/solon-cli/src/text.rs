//! Langue des messages : anglais par défaut, français si `SOLON_LANG`/`LANG` commence par `fr` ou
//! si Windows est en français. Deux langues seulement, donc pas de fichiers de traduction :
//! chaque message est écrit dans les deux à l'endroit où il sert, avec [`tr`].

use std::sync::OnceLock;

static FRENCH: OnceLock<bool> = OnceLock::new();
/// Page de codes de la console avant nous, remise en place à la sortie (0 : rien à remettre).
static PREVIOUS_CP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn french() -> bool {
    *FRENCH.get_or_init(detect)
}

fn detect() -> bool {
    for var in ["SOLON_LANG", "LC_ALL", "LANG"] {
        if let Ok(v) = std::env::var(var)
            && !v.is_empty()
        {
            return v.to_lowercase().starts_with("fr");
        }
    }
    #[cfg(windows)]
    {
        // SAFETY : appel sans argument ni pointeur, rend l'identifiant de langue de l'interface.
        let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        return (id & 0x3ff) == 0x0c; // LANG_FRENCH
    }
    #[allow(unreachable_code)]
    false
}

/// Le texte dans la langue de l'utilisateur.
pub fn tr<'a>(en: &'a str, fr: &'a str) -> &'a str {
    if french() { fr } else { en }
}

/// Sortie console en UTF-8 : sans cela, `cmd.exe` affiche les accents dans la page de codes ANSI.
pub fn utf8_console() {
    #[cfg(windows)]
    {
        use windows::Win32::System::Console::{GetConsoleOutputCP, SetConsoleOutputCP};
        // SAFETY : lit puis change seulement la page de codes de sortie de la console courante.
        let previous = unsafe { GetConsoleOutputCP() };
        if previous != 0 && previous != 65001 {
            PREVIOUS_CP.store(previous, std::sync::atomic::Ordering::Relaxed);
            // SAFETY : idem.
            let _ = unsafe { SetConsoleOutputCP(65001) };
        }
    }
}

/// Remet la page de codes trouvée au départ : à appeler avant toute sortie du programme, car
/// `std::process::exit` ne déroule rien.
pub fn restore_console() {
    #[cfg(windows)]
    {
        let previous = PREVIOUS_CP.swap(0, std::sync::atomic::Ordering::Relaxed);
        if previous != 0 {
            // SAFETY : remet la valeur lue par `utf8_console`.
            let _ = unsafe { windows::Win32::System::Console::SetConsoleOutputCP(previous) };
        }
    }
}
