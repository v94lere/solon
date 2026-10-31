//! Petites choses partagées par les commandes : arguments, durées, heures, tableaux.

use std::collections::{HashMap, HashSet};

use chrono::{Local, TimeZone};

use crate::text::tr;

/// Arguments d'une commande : positionnels, drapeaux (`--json`), valeurs (`--last 20`, `--name=x`).
pub struct Args {
    pub positional: Vec<String>,
    flags: HashSet<String>,
    values: HashMap<String, String>,
}

/// Options qui prennent une valeur dans le mot suivant.
const WITH_VALUE: &[&str] = &["--last", "-n", "--name", "--project", "-p"];

impl Args {
    pub fn parse(raw: &[String]) -> Args {
        let mut a = Args {
            positional: Vec::new(),
            flags: HashSet::new(),
            values: HashMap::new(),
        };
        let mut i = 0;
        while i < raw.len() {
            let w = raw[i].as_str();
            if let Some((k, v)) = w.split_once('=')
                && k.starts_with('-')
            {
                a.values.insert(k.to_owned(), v.to_owned());
            } else if WITH_VALUE.contains(&w) {
                if let Some(v) = raw.get(i + 1) {
                    a.values.insert(w.to_owned(), v.clone());
                    i += 1;
                }
            } else if w.starts_with('-') && w.len() > 1 {
                a.flags.insert(w.to_owned());
            } else {
                a.positional.push(w.to_owned());
            }
            i += 1;
        }
        a
    }

    pub fn flag(&self, name: &str) -> bool {
        self.flags.contains(name)
    }

    pub fn json(&self) -> bool {
        self.flag("--json")
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// `--last N` / `-n N`, sinon `default`.
    pub fn last(&self, default: usize) -> usize {
        self.value("--last")
            .or_else(|| self.value("-n"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    pub fn pos(&self, i: usize) -> Option<&str> {
        self.positional.get(i).map(String::as_str)
    }
}

/// Heure locale `HH:MM:SS` d'un horodatage en millisecondes Unix.
pub fn clock(ms: u64) -> String {
    Local
        .timestamp_millis_opt(ms as i64)
        .single()
        .map(|d| d.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".to_owned())
}

/// Horodatage pour un nom de fichier : `20261031-1422`.
pub fn file_stamp() -> String {
    Local::now().format("%Y%m%d-%H%M").to_string()
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Durée lisible : `43 ms`, `1.2 s`, `2 min 05 s`, `3 h 12 min`.
pub fn dur(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{} min {:02} s", ms / 60_000, (ms % 60_000) / 1000)
    } else {
        format!("{} h {:02} min", ms / 3_600_000, (ms % 3_600_000) / 60_000)
    }
}

/// `1.2 GB` / `1,2 Go`.
pub fn bytes(b: u64) -> String {
    let (unit_en, unit_fr, div) = if b >= 1 << 30 {
        ("GB", "Go", (1u64 << 30) as f64)
    } else if b >= 1 << 20 {
        ("MB", "Mo", (1u64 << 20) as f64)
    } else if b >= 1 << 10 {
        ("KB", "Ko", (1u64 << 10) as f64)
    } else {
        return format!("{b} {}", tr("bytes", "octets"));
    };
    let n = b as f64 / div;
    let s = format!("{n:.1}");
    if crate::text::french() {
        format!("{} {unit_fr}", s.replace('.', ","))
    } else {
        format!("{s} {unit_en}")
    }
}

/// Tableau aligné à gauche, deux espaces entre colonnes, première ligne = en-têtes.
pub fn table(rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; cols];
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count());
        }
    }
    for r in rows {
        let mut line = String::new();
        for (i, c) in r.iter().enumerate() {
            line.push_str(c);
            if i + 1 < r.len() {
                let pad = widths[i] - c.chars().count() + 2;
                line.extend(std::iter::repeat_n(' ', pad));
            }
        }
        out!("{}", line.trim_end());
    }
}

/// Chemin de la machine → chemin Windows quand il vient d'un lecteur partagé
/// (`/mnt/host/c/Users/x` → `C:\Users\x`). Les autres sont rendus tels quels.
pub fn host_path(guest: &str) -> String {
    if let Some(rest) = guest.strip_prefix("/mnt/host/")
        && let Some((drive, tail)) = rest.split_once('/').or(Some((rest, "")))
        && drive.len() == 1
    {
        return format!("{}:\\{}", drive.to_uppercase(), tail.replace('/', "\\"));
    }
    guest.to_owned()
}

pub fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("{} {msg}", tr("error:", "erreur :"));
    exit(1)
}

/// Sortie du programme qui remet la console comme on l'a trouvée.
pub fn exit(code: i32) -> ! {
    crate::text::restore_console();
    std::process::exit(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments() {
        let raw: Vec<String> = ["blog", "--json", "--last", "20", "--name=x", "-f"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let a = Args::parse(&raw);
        assert_eq!(a.positional, vec!["blog"]);
        assert!(a.json());
        assert!(a.flag("-f"));
        assert_eq!(a.last(50), 20);
        assert_eq!(a.value("--name"), Some("x"));
    }

    #[test]
    fn durees_et_chemins() {
        assert_eq!(dur(43), "43 ms");
        assert_eq!(dur(1234), "1.2 s");
        assert_eq!(dur(125_000), "2 min 05 s");
        assert_eq!(host_path("/mnt/host/c/Users/x/blog"), r"C:\Users\x\blog");
        assert_eq!(host_path("/mnt/host/d"), r"D:\");
        assert_eq!(host_path("/srv/app"), "/srv/app");
    }
}
