//! Emplacements sur disque du service. Tout vit sous `%ProgramData%\Monodon` (lisible par le
//! service LocalSystem et, en lecture, par les utilisateurs pour les journaux).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    pub fn default_root() -> PathBuf {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        let root = base.join("Monodon");
        // Installation datant d'avant le renommage du projet (Solon, 8 septembre 2026) : les données
        // (disque, réglages, journaux, autorité) sont reprises entrée par entrée, sans écraser ce que
        // Monodon aurait déjà créé (l'installeur écrit son journal dans Monodon\logs avant ce démarrage).
        let legacy = base.join("Solon");
        if legacy.is_dir() && !root.join("data.vhdx").exists() {
            migrate_legacy_dir(&legacy, &root);
        }
        root
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn image_dir(&self) -> PathBuf {
        self.root.join("image")
    }
    pub fn data_disk(&self) -> PathBuf {
        self.root.join("data.vhdx")
    }
    pub fn state_file(&self) -> PathBuf {
        self.root.join("state.json")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }
}

/// Manifeste produit par `image/build.sh`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageManifest {
    pub schema: u32,
    pub version: String,
    pub built_at: String,
    pub kernel: ImageFile,
    pub initrd: ImageFile,
    pub rootfs: ImageFile,
    #[serde(default)]
    pub docker_engine: String,
    pub kernel_cmdline: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageFile {
    pub file: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub release: Option<String>,
}

/// Image résolue sur disque, prête à démarrer.
#[derive(Debug, Clone)]
pub struct ResolvedImage {
    pub dir: PathBuf,
    pub manifest: ImageManifest,
}

impl ResolvedImage {
    pub fn kernel(&self) -> PathBuf {
        self.dir.join(&self.manifest.kernel.file)
    }
    pub fn initrd(&self) -> PathBuf {
        self.dir.join(&self.manifest.initrd.file)
    }
    pub fn rootfs(&self) -> PathBuf {
        self.dir.join(&self.manifest.rootfs.file)
    }

    /// Charge `manifest.json` dans `dir` et vérifie tailles et SHA-256 des trois fichiers.
    pub fn load_verified(dir: &Path) -> Result<Self, String> {
        let manifest_path = dir.join("manifest.json");
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("{} : {e}", manifest_path.display()))?;
        let manifest: ImageManifest =
            serde_json::from_str(&text).map_err(|e| format!("manifest.json illisible : {e}"))?;
        if manifest.schema != 1 {
            return Err(format!(
                "schéma de manifeste {} non pris en charge",
                manifest.schema
            ));
        }
        let img = Self {
            dir: dir.to_path_buf(),
            manifest,
        };
        for (label, file, path) in [
            ("noyau", &img.manifest.kernel, img.kernel()),
            ("initrd", &img.manifest.initrd, img.initrd()),
            ("système racine", &img.manifest.rootfs, img.rootfs()),
        ] {
            let meta = std::fs::metadata(&path)
                .map_err(|e| format!("{label} absent ({}) : {e}", path.display()))?;
            if meta.len() != file.size {
                return Err(format!(
                    "{label} : taille {} au lieu de {}",
                    meta.len(),
                    file.size
                ));
            }
            let digest =
                sha256_file(&path).map_err(|e| format!("{label} : lecture impossible : {e}"))?;
            if !digest.eq_ignore_ascii_case(&file.sha256) {
                return Err(format!(
                    "{label} : empreinte SHA-256 invalide (fichier corrompu ou modifié)"
                ));
            }
        }
        Ok(img)
    }
}

/// SHA-256 d'un fichier, implémentation locale (évite une dépendance pour un seul usage).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finish_hex())
}

/// Implémentation minimale de SHA-256 (FIPS 180-4).
mod sha2 {
    pub struct Sha256 {
        state: [u32; 8],
        buffer: Vec<u8>,
        length: u64,
    }

    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    impl Sha256 {
        pub fn new() -> Self {
            Self {
                state: [
                    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                    0x1f83d9ab, 0x5be0cd19,
                ],
                buffer: Vec::with_capacity(128),
                length: 0,
            }
        }

        pub fn update(&mut self, data: &[u8]) {
            self.length += data.len() as u64;
            self.buffer.extend_from_slice(data);
            let full = self.buffer.len() / 64 * 64;
            for chunk in self.buffer[..full].chunks_exact(64) {
                let mut w = [0u32; 64];
                for (i, word) in chunk.chunks_exact(4).enumerate() {
                    w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
                }
                for i in 16..64 {
                    let s0 =
                        w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                    let s1 =
                        w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                    w[i] = w[i - 16]
                        .wrapping_add(s0)
                        .wrapping_add(w[i - 7])
                        .wrapping_add(s1);
                }
                let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
                for i in 0..64 {
                    let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                    let ch = (e & f) ^ (!e & g);
                    let t1 = h
                        .wrapping_add(s1)
                        .wrapping_add(ch)
                        .wrapping_add(K[i])
                        .wrapping_add(w[i]);
                    let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                    let maj = (a & b) ^ (a & c) ^ (b & c);
                    let t2 = s0.wrapping_add(maj);
                    h = g;
                    g = f;
                    f = e;
                    e = d.wrapping_add(t1);
                    d = c;
                    c = b;
                    b = a;
                    a = t1.wrapping_add(t2);
                }
                for (s, v) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
                    *s = s.wrapping_add(v);
                }
            }
            self.buffer.drain(..full);
        }

        pub fn finish_hex(mut self) -> String {
            let bit_len = self.length * 8;
            let mut pad = vec![0x80u8];
            while (self.buffer.len() + pad.len()) % 64 != 56 {
                pad.push(0);
            }
            pad.extend_from_slice(&bit_len.to_be_bytes());
            let remaining = self.length;
            self.update(&pad);
            self.length = remaining;
            self.state.iter().map(|w| format!("{w:08x}")).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sha2::Sha256;

    #[test]
    fn sha256_vecteurs_connus() {
        let mut h = Sha256::new();
        h.update(b"");
        assert_eq!(
            h.finish_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let mut h = Sha256::new();
        h.update(b"abc");
        assert_eq!(
            h.finish_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let mut h = Sha256::new();
        h.update(&[b'a'; 1000]);
        h.update(&[b'a'; 24]);
        // 1024 × 'a'
        assert_eq!(
            h.finish_hex(),
            "2edc986847e209b4016e141a6dc8716d3207350f416969382d431539bf292e4a"
        );
    }
}

/// Déplace le contenu de `from` dans `to` (fusion des sous-dossiers, jamais d'écrasement), puis
/// supprime `from` s'il est vide. Même volume : de simples renommages, instantanés.
fn migrate_legacy_dir(from: &std::path::Path, to: &std::path::Path) {
    let _ = std::fs::create_dir_all(to);
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    let mut moved = 0usize;
    for entry in entries.flatten() {
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() && dst.is_dir() {
            migrate_legacy_dir(&src, &dst);
            continue;
        }
        if dst.exists() {
            continue;
        }
        match std::fs::rename(&src, &dst) {
            Ok(()) => moved += 1,
            Err(e) => tracing::warn!("reprise de {} impossible : {e}", src.display()),
        }
    }
    if moved > 0 {
        tracing::info!(moved, "données reprises depuis {}", from.display());
    }
    let _ = std::fs::remove_dir(from);
}
