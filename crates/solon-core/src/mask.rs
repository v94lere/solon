//! Masquage des informations sensibles avant qu'elles n'entrent dans la radiographie.
//!
//! La règle est volontairement brutale : on masque dès qu'il y a un doute. Un en-tête masqué à
//! tort coûte une gêne ; un jeton de session lisible dans un historique gardé une heure, et
//! visible par quiconque regarde l'écran, coûte beaucoup plus cher.
//!
//! Le masquage a lieu **avant** le stockage, jamais à l'affichage : ce qui n'est pas capturé ne
//! peut pas fuir, même si l'historique est relu ou exporté plus tard.
//!
//! Aucune expression rationnelle ici. Les motifs recherchés sont simples, et les écrire à la main
//! évite une dépendance de plus dans une caisse que l'agent invité embarque aussi.

/// Ce qui remplace une valeur masquée. Le nom de l'en-tête ou du paramètre, lui, reste visible :
/// savoir qu'un `Authorization` était présent fait partie du débogage.
pub const REDACTED: &str = "•••";

/// En-têtes dont la valeur ne sort jamais, quelle qu'elle soit.
const SECRET_HEADERS: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "cookie",
    "set-cookie",
    "authentication",
    "www-authenticate",
    "proxy-authenticate",
    "x-api-key",
    "api-key",
    "x-auth-token",
    "x-access-token",
    "x-csrf-token",
    "x-xsrf-token",
    "x-session-token",
];

/// Fragments qui, dans le nom d'un paramètre d'URL, suffisent à masquer sa valeur. Larges exprès :
/// `key` attrape `keyword` au passage, et c'est le bon côté sur lequel se tromper.
const SECRET_PARAMS: &[&str] = &[
    "pass",
    // WordPress envoie son mot de passe sous `pwd`, et des formulaires français sous `mdp`.
    "pwd",
    "mdp",
    "token",
    "secret",
    "key",
    "auth",
    "session",
    "sig",
    "credential",
    "otp",
    "code",
];

pub fn is_secret_header(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    SECRET_HEADERS.contains(&lower.as_str())
}

/// Valeur d'en-tête telle qu'elle sera gardée.
pub fn header_value(name: &str, value: &str) -> String {
    if is_secret_header(name) {
        return REDACTED.to_owned();
    }
    text(value)
}

/// Chemin d'une requête, avec sa chaîne de paramètres. Les paramètres au nom suspect perdent leur
/// valeur ; le reste passe par le masquage du texte libre.
pub fn path(path: &str) -> String {
    let Some((before, query)) = path.split_once('?') else {
        return text(path);
    };
    let mut out = String::with_capacity(path.len());
    out.push_str(&text(before));
    out.push('?');
    for (i, pair) in query.split('&').enumerate() {
        if i > 0 {
            out.push('&');
        }
        match pair.split_once('=') {
            Some((name, value)) => {
                out.push_str(name);
                out.push('=');
                if is_secret_param(name) {
                    out.push_str(REDACTED);
                } else {
                    out.push_str(&text(value));
                }
            }
            None => out.push_str(pair),
        }
    }
    out
}

fn is_secret_param(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SECRET_PARAMS.iter().any(|f| lower.contains(f))
}

/// Types de contenu dont le corps vaut la peine d'être montré. Tout le reste — images, archives,
/// polices, flux binaires — n'apprend rien à l'écran et n'est pas capturé du tout.
pub fn is_text_content(content_type: &str) -> bool {
    let mime = mime_of(content_type);
    mime.starts_with("text/")
        || mime.ends_with("+json")
        || mime.ends_with("+xml")
        || matches!(
            mime.as_str(),
            "application/json"
                | "application/xml"
                | "application/x-www-form-urlencoded"
                | "application/javascript"
                | "application/x-ndjson"
                | "application/graphql"
        )
}

fn mime_of(content_type: &str) -> String {
    content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// Un corps tel qu'il sera gardé : décodé en texte, puis masqué selon sa forme.
///
/// Un formulaire (`application/x-www-form-urlencoded`) est traité comme une chaîne de paramètres :
/// les champs au nom suspect perdent leur valeur. Un document JSON perd les valeurs des clés au
/// nom suspect, à quelque profondeur qu'elles soient. Le reste passe par le masquage du texte
/// libre. Dans tous les cas, courriels et téléphones disparaissent.
pub fn body(content_type: &str, bytes: &[u8]) -> String {
    let raw = String::from_utf8_lossy(bytes);
    let mime = mime_of(content_type);
    if mime == "application/x-www-form-urlencoded" {
        return form(&raw);
    }
    if mime == "application/json" || mime.ends_with("+json") || mime == "application/x-ndjson" {
        return text(&json_values(&raw));
    }
    text(&raw)
}

/// Une chaîne `a=1&password=x` : même règle que les paramètres d'URL.
fn form(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, pair) in s.split('&').enumerate() {
        if i > 0 {
            out.push('&');
        }
        match pair.split_once('=') {
            Some((name, value)) => {
                out.push_str(name);
                out.push('=');
                if is_secret_param(name) {
                    out.push_str(REDACTED);
                } else {
                    out.push_str(&text(value));
                }
            }
            None => out.push_str(pair),
        }
    }
    out
}

/// Dans un texte JSON, remplace la valeur (chaîne, nombre ou mot-clé) de toute clé au nom suspect.
///
/// Pas d'analyse JSON complète : un balayage des chaînes suffit. Une chaîne suivie de `:` est une
/// clé ; si son nom est suspect, la valeur qui suit est remplacée — une chaîne en entier, virgules
/// et deux-points compris, un nombre ou un mot-clé jusqu'au séparateur. Un objet ou un tableau en
/// valeur n'est pas touché : ses propres clés passent au balayage.
fn json_values(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'"' {
            let start = i;
            while i < b.len() && b[i] != b'"' {
                i += 1;
            }
            out.push_str(&s[start..i]);
            continue;
        }
        let Some(close) = json_string_end(b, i) else {
            out.push_str(&s[i..]);
            break;
        };
        let key = &s[i + 1..close];
        out.push_str(&s[i..=close]);
        i = close + 1;
        // Une clé est suivie de `:` (espaces permis).
        let mut j = i;
        while j < b.len() && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= b.len() || b[j] != b':' || !is_secret_param(key) {
            continue;
        }
        out.push_str(&s[i..=j]);
        i = j + 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            out.push(' ');
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        match b[i] {
            b'"' => {
                let end = json_string_end(b, i).unwrap_or(b.len() - 1);
                out.push('"');
                out.push_str(REDACTED);
                out.push('"');
                i = end + 1;
            }
            b'{' | b'[' => {}
            _ => {
                while i < b.len()
                    && !matches!(b[i], b',' | b'}' | b']')
                    && !b[i].is_ascii_whitespace()
                {
                    i += 1;
                }
                out.push_str(REDACTED);
            }
        }
    }
    out
}

/// Indice du guillemet qui ferme la chaîne JSON ouverte en `open`, en sautant les échappements.
fn json_string_end(b: &[u8], open: usize) -> Option<usize> {
    let mut i = open + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Remplace dans un texte libre ce qui désigne une personne : adresses de courriel et numéros de
/// téléphone.
pub fn text(s: &str) -> String {
    let phones = mask_phones(s);
    mask_emails(&phones)
}

/// Repère les adresses de courriel en partant des `@` et en s'étendant de part et d'autre.
///
/// Plus sûr que de découper le texte en mots : une adresse se retrouve aussi bien au milieu d'un
/// chemin que dans une valeur d'en-tête, sans espace autour.
fn mask_emails(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut copied = 0usize;
    for at in 0..b.len() {
        if b[at] != b'@' || at < copied {
            continue;
        }
        let mut start = at;
        while start > copied && is_local_byte(b[start - 1]) {
            start -= 1;
        }
        let mut end = at + 1;
        while end < b.len() && is_domain_byte(b[end]) {
            end += 1;
        }
        if start == at || !looks_like_domain(&s[at + 1..end]) {
            continue;
        }
        out.push_str(&s[copied..start]);
        out.push_str(REDACTED);
        copied = end;
    }
    out.push_str(&s[copied..]);
    out
}

fn is_local_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'%' | b'+' | b'-')
}

fn is_domain_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')
}

/// Un domaine plausible : au moins un point, et une extension d'au moins deux lettres.
fn looks_like_domain(s: &str) -> bool {
    match s.rsplit_once('.') {
        Some((before, tld)) => {
            !before.is_empty() && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
        }
        None => false,
    }
}

/// Repère les numéros de téléphone, et seulement eux.
///
/// Deux formes, choisies étroites : un `+` suivi de 8 à 15 chiffres, et un `0` suivi de 9 chiffres
/// — l'écriture française. Une longue suite de chiffres quelconque n'est **pas** masquée : ce
/// serait avaler les horodatages, les tailles et les identifiants, dont le débogage a besoin.
fn mask_phones(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < b.len() {
        if let Some(end) = phone_end(b, i) {
            out.push_str(REDACTED);
            i = end;
            continue;
        }
        // Les octets d'un caractère multioctet se recopient tels quels : on n'y cherche rien.
        let start = i;
        i += 1;
        while i < b.len() && (b[i] & 0xC0) == 0x80 {
            i += 1;
        }
        out.push_str(&s[start..i]);
    }
    out
}

/// Si un numéro commence à `i`, renvoie l'indice juste après. Le chiffre qui précède disqualifie :
/// sans cela, la fin d'un identifiant numérique passerait pour un numéro.
fn phone_end(b: &[u8], i: usize) -> Option<usize> {
    if i > 0 && b[i - 1].is_ascii_digit() {
        return None;
    }
    let international = b[i] == b'+';
    if !international && b[i] != b'0' {
        return None;
    }
    let mut digits = usize::from(!international);
    let mut end = i + 1;
    while end < b.len() {
        match b[end] {
            d if d.is_ascii_digit() => {
                digits += 1;
                end += 1;
            }
            // Séparateurs acceptés, mais seulement entre deux chiffres.
            b' ' | b'.' | b'-' if end + 1 < b.len() && b[end + 1].is_ascii_digit() => end += 1,
            _ => break,
        }
    }
    if end < b.len() && b[end].is_ascii_digit() {
        return None;
    }
    let plausible = if international {
        (8..=15).contains(&digits)
    } else {
        digits == 10
    };
    plausible.then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn en_tetes_secrets_reconnus_sans_casse() {
        assert!(is_secret_header("Authorization"));
        assert!(is_secret_header("  set-cookie "));
        assert!(is_secret_header("X-Api-Key"));
        assert!(!is_secret_header("content-type"));
        assert!(!is_secret_header("x-request-id"));
    }

    #[test]
    fn la_valeur_d_un_en_tete_secret_ne_sort_pas() {
        assert_eq!(
            header_value("Authorization", "Bearer abc.def.ghi"),
            REDACTED
        );
        assert_eq!(
            header_value("Content-Type", "application/json"),
            "application/json"
        );
    }

    #[test]
    fn parametres_suspects_masques() {
        assert_eq!(
            path("/login?user=marc&password=hunter2&next=/accueil"),
            "/login?user=marc&password=•••&next=/accueil"
        );
        assert_eq!(path("/api?api_key=zzz&page=2"), "/api?api_key=•••&page=2");
        // Vu sur l'installation réelle : WordPress appelle son mot de passe `pwd`.
        assert_eq!(
            form("log=marc&pwd=hunter2&redirect_to=%2Fwp-admin%2F"),
            "log=marc&pwd=•••&redirect_to=%2Fwp-admin%2F"
        );
        // Pas de chaîne de paramètres : rien à découper.
        assert_eq!(path("/produits/42"), "/produits/42");
        // Un paramètre sans valeur reste tel quel.
        assert_eq!(path("/x?debug"), "/x?debug");
    }

    #[test]
    fn adresses_de_courriel_masquees_partout() {
        assert_eq!(
            text("contact : marc.dupont@example.com !"),
            "contact : ••• !"
        );
        assert_eq!(path("/users/jean@societe.fr/edit"), "/users/•••/edit");
        // Deux adresses dans la même valeur.
        assert_eq!(text("a@b.com, c@d.org"), "•••, •••");
    }

    #[test]
    fn ce_qui_ressemble_a_une_adresse_sans_en_etre_passe() {
        // Pas de partie locale.
        assert_eq!(text("@solon"), "@solon");
        // Pas d'extension alphabétique.
        assert_eq!(text("marc@192.168.1.1"), "marc@192.168.1.1");
        // Pas de point du tout.
        assert_eq!(text("marc@localhost"), "marc@localhost");
    }

    #[test]
    fn numeros_de_telephone_masques() {
        assert_eq!(text("appelle le 06 12 34 56 78"), "appelle le •••");
        assert_eq!(text("0612345678"), "•••");
        assert_eq!(text("+33 6 12 34 56 78"), "•••");
    }

    /// Le débogage vit de nombres : horodatages, tailles, identifiants. Les masquer rendrait
    /// l'outil inutilisable, donc la détection reste étroite.
    #[test]
    fn les_nombres_ordinaires_ne_sont_pas_masques() {
        for s in [
            "1793138555868", // horodatage en millisecondes
            "Content-Length: 94823",
            "/produits/1234567890",
            "id=98765432109",
            "v1.2.3",
        ] {
            assert_eq!(text(s), s, "{s}");
        }
    }

    #[test]
    fn le_masquage_preserve_les_caracteres_accentues() {
        assert_eq!(text("héllo wörld ✓"), "héllo wörld ✓");
        assert_eq!(text("é 0612345678 è"), "é ••• è");
    }

    #[test]
    fn seuls_les_corps_texte_sont_captures() {
        for ct in [
            "application/json",
            "application/json; charset=utf-8",
            "text/html",
            "application/problem+json",
            "application/x-www-form-urlencoded",
            "Text/Plain",
        ] {
            assert!(is_text_content(ct), "{ct}");
        }
        for ct in [
            "image/png",
            "application/octet-stream",
            "application/zip",
            "",
        ] {
            assert!(!is_text_content(ct), "{ct}");
        }
    }

    #[test]
    fn un_formulaire_perd_ses_champs_suspects() {
        assert_eq!(
            body(
                "application/x-www-form-urlencoded",
                b"login=marc&password=hunter2&remember=1"
            ),
            "login=marc&password=•••&remember=1"
        );
    }

    #[test]
    fn un_json_perd_les_valeurs_des_cles_suspectes() {
        let json = br#"{"user":"marc","password":"hunter2","profile":{"api_key": "k1", "age": 42},"tokens":["a","b"],"otp":123456,"ok":true}"#;
        assert_eq!(
            body("application/json", json),
            r#"{"user":"marc","password":"•••","profile":{"api_key": "•••", "age": 42},"tokens":["a","b"],"otp":•••,"ok":true}"#
        );
        // Une chaîne avec des guillemets échappés et un deux-points, qui n'est pas une clé.
        assert_eq!(
            body("application/json", br#"{"msg":"a \"token\": b","n":1}"#),
            r#"{"msg":"a \"token\": b","n":1}"#
        );
        // Un JSON tronqué au milieu d'une chaîne ne fait pas paniquer.
        assert_eq!(
            body("application/json", br#"{"password":"hun"#),
            r#"{"password":"•••""#
        );
        assert_eq!(body("application/json", br#"{"pass"#), r#"{"pass"#);
    }

    #[test]
    fn un_corps_texte_libre_perd_les_courriels() {
        assert_eq!(
            body(
                "text/plain; charset=utf-8",
                "Bonjour marc@example.com".as_bytes()
            ),
            "Bonjour •••"
        );
    }

    #[test]
    fn texte_vide_et_sans_rien_a_masquer() {
        assert_eq!(text(""), "");
        assert_eq!(text("GET /"), "GET /");
    }
}
