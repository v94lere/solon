//! Chaque code d'erreur stable doit avoir un message dans chaque langue de l'interface, et les
//! identifiants de prérequis aussi. Ce test échoue si l'on ajoute un code sans sa traduction.

use solon_core::ErrorCode;

fn locale(name: &str) -> serde_json::Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/locales/");
    let text = std::fs::read_to_string(format!("{path}{name}.json")).expect("fichier de langue");
    serde_json::from_str(&text).expect("JSON de langue valide")
}

#[test]
fn chaque_code_d_erreur_est_traduit() {
    for lng in ["en", "fr"] {
        let l = locale(lng);
        for code in ErrorCode::ALL {
            let key = serde_json::to_value(code).unwrap();
            let key = key.as_str().unwrap();
            assert!(
                l["error"]
                    .get(key)
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.is_empty()),
                "code {key} sans message dans {lng}.json"
            );
        }
    }
}

#[test]
fn chaque_prerequis_est_traduit() {
    for lng in ["en", "fr"] {
        let l = locale(lng);
        for id in [
            "windows_edition",
            "virtualization_firmware",
            "service_vmcompute",
            "service_hns",
            "hcs_api",
            "memory",
        ] {
            assert!(
                l["prereq"].get(id).is_some(),
                "prérequis {id} sans libellé dans {lng}.json"
            );
        }
    }
}

#[test]
fn les_deux_langues_ont_les_memes_cles() {
    fn keys(prefix: &str, v: &serde_json::Value, out: &mut Vec<String>) {
        if let Some(m) = v.as_object() {
            for (k, v) in m {
                keys(&format!("{prefix}{k}."), v, out);
            }
        } else {
            out.push(prefix.trim_end_matches('.').to_owned());
        }
    }
    let (mut en, mut fr) = (Vec::new(), Vec::new());
    keys("", &locale("en"), &mut en);
    keys("", &locale("fr"), &mut fr);
    en.sort();
    fr.sort();
    let missing_fr: Vec<_> = en.iter().filter(|k| !fr.contains(k)).collect();
    let missing_en: Vec<_> = fr.iter().filter(|k| !en.contains(k)).collect();
    assert!(
        missing_fr.is_empty() && missing_en.is_empty(),
        "clés manquantes — fr : {missing_fr:?} ; en : {missing_en:?}"
    );
}
