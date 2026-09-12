//! Recherche des projets Docker présents sur le PC. Sur les volumes NTFS, la table des fichiers (MFT) est
//! lue d'un bloc par `FSCTL_ENUM_USN_DATA` (ce que font les outils du type « Everything ») : un disque d'un
//! million de fichiers en une à trois secondes, sans ouvrir un dossier ; il faut les droits administrateur,
//! que le service a. Ailleurs (exFAT, échec), repli sur un parcours des dossiers probables, borné en
//! profondeur et en temps. Les chemins de dépendances et de système sont écartés.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use solon_core::ipc::FoundProject;

/// Fichiers qui signalent un projet ; `devcontainer.json` vit dans `.devcontainer/`, le projet est au-dessus.
const MARKERS: &[(&str, &str)] = &[
    ("compose.yaml", "compose"),
    ("compose.yml", "compose"),
    ("docker-compose.yml", "compose"),
    ("docker-compose.yaml", "compose"),
    ("dockerfile", "dockerfile"),
    ("devcontainer.json", "devcontainer"),
];

/// Longueurs (en caractères) des noms de repères : filtre avant de décoder un nom.
const MARKER_LENGTHS: &[usize] = &[10, 11, 12, 17, 18, 19];

/// Segments de chemin qui disqualifient un résultat (dépendances, système, caches, sorties de construction).
const EXCLUDED_SEGMENTS: &[&str] = &[
    "node_modules",
    ".git",
    "appdata",
    "program files",
    "program files (x86)",
    "programdata",
    "windows",
    "$recycle.bin",
    "vendor",
    "site-packages",
    ".cargo",
    ".rustup",
    ".npm",
    ".nuget",
    ".cache",
    ".vscode-server",
    "recovery",
    "system volume information",
];

fn excluded(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower
        .split('\\')
        .skip(1)
        .any(|seg| EXCLUDED_SEGMENTS.contains(&seg))
}

fn marker_kind(file_name: &str) -> Option<&'static str> {
    let lower = file_name.to_lowercase();
    MARKERS.iter().find(|(m, _)| *m == lower).map(|(_, k)| *k)
}

/// Ajoute une découverte à `out` (un résultat par dossier, sortes cumulées).
fn record(out: &mut HashMap<String, FoundProject>, dir: String, kind: &'static str, file: String) {
    if excluded(&dir) {
        return;
    }
    let e = out
        .entry(dir.to_lowercase())
        .or_insert_with(|| FoundProject {
            dir: dir.clone(),
            kinds: Vec::new(),
            files: Vec::new(),
            git: Path::new(&dir).join(".git").exists(),
        });
    if !e.kinds.iter().any(|k| k == kind) {
        e.kinds.push(kind.to_owned());
    }
    if !e.files.contains(&file) {
        e.files.push(file);
    }
}

// ---------------------------------------------------------------------------------------------
// Volumes NTFS : lecture de la MFT
// ---------------------------------------------------------------------------------------------

fn fixed_drives() -> Vec<(char, bool)> {
    use windows::Win32::Storage::FileSystem::{
        GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
    };
    use windows::core::HSTRING;
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = (b'A' + i as u8) as char;
        let root = format!("{letter}:\\");
        // DRIVE_FIXED = 3 : disques internes et SSD externes ; ni réseau, ni amovible, ni CD.
        if unsafe { GetDriveTypeW(&HSTRING::from(root.as_str())) } != 3 {
            continue;
        }
        let mut fs = [0u16; 32];
        let ntfs = unsafe {
            GetVolumeInformationW(
                &HSTRING::from(root.as_str()),
                None,
                None,
                None,
                None,
                Some(&mut fs),
            )
        }
        .is_ok()
            && String::from_utf16_lossy(&fs).trim_end_matches('\0') == "NTFS";
        out.push((letter, ntfs));
    }
    out
}

/// Parcourt la MFT du volume `letter` et renvoie les dossiers de projet trouvés.
fn scan_ntfs(
    letter: char,
    deadline: Instant,
    out: &mut HashMap<String, FoundProject>,
) -> Result<(), String> {
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::{FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V0, USN_RECORD_V2};
    use windows::core::HSTRING;

    let handle = unsafe {
        CreateFileW(
            &HSTRING::from(format!("\\\\.\\{letter}:")),
            GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    }
    .map_err(|e| format!("ouverture du volume {letter}: : {e}"))?;

    // Dossiers : numéro → (parent, nom) ; fichiers repères : (parent, nom, sorte).
    let mut dirs: HashMap<u64, (u64, String)> = HashMap::with_capacity(262_144);
    let mut hits: Vec<(u64, String, &'static str)> = Vec::new();
    let mut input = MFT_ENUM_DATA_V0 {
        StartFileReferenceNumber: 0,
        LowUsn: 0,
        HighUsn: i64::MAX,
    };
    let mut buf = vec![0u8; 4 << 20];
    let result: Result<(), String> = loop {
        if Instant::now() > deadline {
            break Err(format!("volume {letter}: : délai dépassé"));
        }
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_ENUM_USN_DATA,
                Some(&input as *const _ as *const _),
                std::mem::size_of::<MFT_ENUM_DATA_V0>() as u32,
                Some(buf.as_mut_ptr() as *mut _),
                buf.len() as u32,
                Some(&mut returned),
                None,
            )
        };
        if let Err(e) = ok {
            // ERROR_HANDLE_EOF (38) : fin de l'énumération.
            if e.code().0 as u32 & 0xffff == 38 {
                break Ok(());
            }
            break Err(format!("énumération MFT {letter}: : {e}"));
        }
        if returned < 8 {
            break Ok(());
        }
        input.StartFileReferenceNumber = u64::from_ne_bytes(buf[..8].try_into().unwrap());
        let mut off = 8usize;
        while off + std::mem::size_of::<USN_RECORD_V2>() <= returned as usize {
            let rec: USN_RECORD_V2 =
                unsafe { std::ptr::read_unaligned(buf[off..].as_ptr() as *const USN_RECORD_V2) };
            let len = rec.RecordLength as usize;
            if len == 0 || off + len > returned as usize {
                break;
            }
            let name_off = off + rec.FileNameOffset as usize;
            let name_len = rec.FileNameLength as usize / 2;
            const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
            let is_dir = rec.FileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
            // Les fichiers ordinaires sont l'immense majorité : leur nom n'est décodé que si sa longueur
            // est celle d'un repère (10 à 19 caractères), ce qui divise le travail par dix.
            if is_dir || MARKER_LENGTHS.contains(&name_len) {
                let name: String = char::decode_utf16((0..name_len).map(|i| {
                    u16::from_ne_bytes([buf[name_off + 2 * i], buf[name_off + 2 * i + 1]])
                }))
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect();
                if is_dir {
                    dirs.insert(
                        rec.FileReferenceNumber,
                        (rec.ParentFileReferenceNumber, name),
                    );
                } else if let Some(kind) = marker_kind(&name) {
                    hits.push((rec.ParentFileReferenceNumber, name, kind));
                }
            }
            off += len;
        }
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    result?;

    // Chemin complet d'un dossier : remontée des parents jusqu'à la racine (dont le parent est elle-même).
    let mut cache: HashMap<u64, Option<String>> = HashMap::new();
    fn path_of(
        frn: u64,
        letter: char,
        dirs: &HashMap<u64, (u64, String)>,
        cache: &mut HashMap<u64, Option<String>>,
        depth: usize,
    ) -> Option<String> {
        if let Some(p) = cache.get(&frn) {
            return p.clone();
        }
        if depth > 64 {
            return None;
        }
        // La racine (numéro 5) n'est pas renvoyée par l'énumération : un parent absent qui porte ce
        // numéro est la racine du lecteur.
        let Some((parent, name)) = dirs.get(&frn) else {
            return if frn & 0x0000_FFFF_FFFF_FFFF == 5 {
                Some(format!("{letter}:"))
            } else {
                None
            };
        };
        let path = if *parent == frn || name == "." {
            format!("{letter}:")
        } else {
            let base = path_of(*parent, letter, dirs, cache, depth + 1)?;
            format!("{base}\\{name}")
        };
        cache.insert(frn, Some(path.clone()));
        Some(path)
    }
    for (parent, file, kind) in hits {
        let Some(dir) = path_of(parent, letter, &dirs, &mut cache, 0) else {
            continue;
        };
        // `.devcontainer/devcontainer.json` : le projet est le dossier au-dessus.
        let dir = if kind == "devcontainer" {
            match Path::new(&dir).parent() {
                Some(p) if dir.to_lowercase().ends_with("\\.devcontainer") => {
                    p.to_string_lossy().into_owned()
                }
                _ => continue,
            }
        } else {
            dir
        };
        if dir.len() <= 2 {
            continue; // racine du lecteur : jamais un projet
        }
        record(out, dir, kind, file);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Repli : parcours des dossiers probables
// ---------------------------------------------------------------------------------------------

fn walk(dir: &Path, depth: usize, deadline: Instant, out: &mut HashMap<String, FoundProject>) {
    if depth > 6 || Instant::now() > deadline {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if EXCLUDED_SEGMENTS.contains(&name.to_lowercase().as_str()) || ft.is_symlink() {
                continue;
            }
            walk(&path, depth + 1, deadline, out);
        } else if let Some(kind) = marker_kind(&name) {
            let mut project = dir.to_path_buf();
            if kind == "devcontainer" {
                if !dir
                    .to_string_lossy()
                    .to_lowercase()
                    .ends_with("\\.devcontainer")
                {
                    continue;
                }
                match dir.parent() {
                    Some(p) => project = p.to_path_buf(),
                    None => continue,
                }
            }
            record(out, project.to_string_lossy().into_owned(), kind, name);
        }
    }
}

fn likely_roots(letter: char) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let drive = PathBuf::from(format!("{letter}:\\"));
    if letter == 'C' {
        if let Ok(users) = std::fs::read_dir(drive.join("Users")) {
            for u in users.flatten() {
                let home = u.path();
                let name = u.file_name().to_string_lossy().to_lowercase();
                if matches!(
                    name.as_str(),
                    "public" | "default" | "default user" | "all users"
                ) {
                    continue;
                }
                for sub in [
                    "Desktop",
                    "Documents",
                    "Downloads",
                    "Projects",
                    "projects",
                    "dev",
                    "src",
                    "code",
                    "repos",
                    "git",
                    "work",
                    "source",
                ] {
                    let p = home.join(sub);
                    if p.is_dir() {
                        roots.push(p);
                    }
                }
            }
        }
    } else {
        roots.push(drive);
    }
    roots
}

// ---------------------------------------------------------------------------------------------

/// Résultat d'une recherche : projets trouvés, lecteurs parcourus, lecteurs en repli ou en échec.
#[derive(Debug, serde::Serialize)]
pub struct ScanReport {
    pub projects: Vec<FoundProject>,
    pub drives: Vec<String>,
    pub notes: Vec<String>,
    pub ms: u64,
}

/// Cherche les projets sur tous les disques internes, dans la limite de `budget`.
pub fn scan_projects(budget: Duration) -> ScanReport {
    let t0 = Instant::now();
    let deadline = t0 + budget;
    let mut out: HashMap<String, FoundProject> = HashMap::new();
    let mut drives = Vec::new();
    let mut notes = Vec::new();
    for (letter, ntfs) in fixed_drives() {
        drives.push(format!("{letter}:"));
        let fast = if ntfs {
            match scan_ntfs(letter, deadline, &mut out) {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!("recherche de projets : {e} ; parcours des dossiers");
                    notes.push(e);
                    false
                }
            }
        } else {
            notes.push(format!(
                "{letter}: n'est pas NTFS : parcours des dossiers probables"
            ));
            false
        };
        if !fast {
            for root in likely_roots(letter) {
                walk(&root, 0, deadline, &mut out);
            }
        }
        if Instant::now() > deadline {
            notes.push("délai atteint : résultats partiels".into());
            break;
        }
    }
    let mut projects: Vec<FoundProject> = out.into_values().collect();
    projects.sort_by(|a, b| {
        let ra = if a.kinds.iter().any(|k| k == "compose") {
            0
        } else {
            1
        };
        let rb = if b.kinds.iter().any(|k| k == "compose") {
            0
        } else {
            1
        };
        ra.cmp(&rb)
            .then_with(|| a.dir.to_lowercase().cmp(&b.dir.to_lowercase()))
    });
    tracing::info!(
        "recherche de projets : {} dossier(s) sur {} lecteur(s) en {} ms",
        projects.len(),
        drives.len(),
        t0.elapsed().as_millis()
    );
    ScanReport {
        projects,
        drives,
        notes,
        ms: t0.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusions_et_reperes() {
        assert!(excluded(r"C:\Users\x\proj\node_modules\pkg"));
        assert!(excluded(r"C:\Program Files\App"));
        assert!(!excluded(r"C:\Users\x\Documents\odoo18"));
        assert_eq!(marker_kind("Compose.YAML"), Some("compose"));
        assert_eq!(marker_kind("Dockerfile"), Some("dockerfile"));
        assert_eq!(marker_kind("Dockerfile.dev"), None);
    }

    #[test]
    fn parcours_du_depot() {
        // Le dépôt Solon contient bench/compose.yaml : le parcours de repli le trouve.
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let mut out = HashMap::new();
        walk(root, 0, Instant::now() + Duration::from_secs(20), &mut out);
        assert!(
            out.values()
                .any(|p| p.dir.to_lowercase().ends_with("\\bench")
                    && p.kinds.contains(&"compose".to_owned()))
        );
    }
}
