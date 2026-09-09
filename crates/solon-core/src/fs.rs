//! Protocole du système de fichiers Solon (« solonfs ») : l'invité (client FUSE dans l'agent)
//! interroge l'hôte (serveur dans le service) pour lire et écrire les dossiers Windows partagés.
//!
//! Pourquoi un protocole maison plutôt que 9P : chaque aller-retour hôte↔invité coûte ~0,5 ms ;
//! 9P en demande un par `stat`. Ici `Readdir` renvoie les **attributs de toutes les entrées** en
//! une seule réponse, `Lookup` renvoie l'attribut complet, et le client met en cache attributs et
//! listes pendant une courte durée : un `find` ou un `stat` massif ne coûte plus qu'un aller-retour
//! par dossier.
//!
//! Trame (dans les deux sens) : `u32 LE taille_json`, `u32 LE taille_données`, JSON, données brutes.
//! Les données brutes portent le contenu lu (réponse `Data`) ou à écrire (requête `Write`).
//!
//! Sens des connexions : c'est l'hôte qui se connecte à l'invité (port vsock [`PORT_FS`]), comme
//! pour tous les autres canaux ; côté protocole c'est l'invité qui envoie les requêtes.

use serde::{Deserialize, Serialize};

pub const PORT_FS: u32 = 5006;
/// Connexions ouvertes par l'hôte vers l'invité (requêtes servies en parallèle).
pub const FS_CONNECTIONS: usize = 4;
/// Taille maximale d'une lecture ou d'une écriture par requête.
pub const FS_MAX_IO: u32 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsKind {
    File,
    Dir,
    Symlink,
}

/// Attributs d'une entrée. Les temps sont en nanosecondes depuis l'époque Unix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsAttr {
    pub kind: FsKind,
    pub size: u64,
    pub mtime_ns: i64,
    pub atime_ns: i64,
    pub ctime_ns: i64,
    /// Lecture seule côté Windows (attribut READONLY).
    pub readonly: bool,
}

/// Requêtes de l'invité. `path` est relatif à la racine des partages : `c/Users/v/proj` désigne
/// `C:\Users\v\proj` ; `c` seul désigne la racine du lecteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FsRequest {
    Lookup {
        parent: String,
        name: String,
    },
    Getattr {
        path: String,
    },
    /// Toutes les entrées d'un dossier avec leurs attributs.
    Readdir {
        path: String,
    },
    Read {
        path: String,
        offset: u64,
        len: u32,
    },
    /// Données dans la section brute de la trame.
    Write {
        path: String,
        offset: u64,
    },
    Create {
        path: String,
    },
    Mkdir {
        path: String,
    },
    Unlink {
        path: String,
    },
    Rmdir {
        path: String,
    },
    Rename {
        from: String,
        to: String,
    },
    Truncate {
        path: String,
        size: u64,
    },
    SetTimes {
        path: String,
        mtime_ns: Option<i64>,
        atime_ns: Option<i64>,
    },
    Fsync {
        path: String,
    },
    Statfs {
        path: String,
    },
    Readlink {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsEntry {
    pub name: String,
    pub attr: FsAttr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsStatfs {
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "r", rename_all = "snake_case")]
pub enum FsResponse {
    Attr {
        attr: FsAttr,
    },
    Entries {
        entries: Vec<FsEntry>,
    },
    /// Le contenu est dans la section brute de la trame.
    Data,
    Written {
        len: u32,
    },
    Link {
        target: String,
    },
    Statfs {
        statfs: FsStatfs,
    },
    Ok,
    /// `errno` Linux (ENOENT, EACCES, EEXIST, ENOTEMPTY, EISDIR, ENOTDIR, EIO…).
    Error {
        errno: i32,
    },
}

/// Correspondance erreur d'E/S → errno Linux, faite côté hôte.
pub fn errno_for(kind: std::io::ErrorKind, raw_os: Option<i32>) -> i32 {
    use std::io::ErrorKind as K;
    match kind {
        K::NotFound => 2,            // ENOENT
        K::PermissionDenied => 13,   // EACCES
        K::AlreadyExists => 17,      // EEXIST
        K::InvalidInput => 22,       // EINVAL
        K::DirectoryNotEmpty => 39,  // ENOTEMPTY
        K::IsADirectory => 21,       // EISDIR
        K::NotADirectory => 20,      // ENOTDIR
        K::ReadOnlyFilesystem => 30, // EROFS
        K::StorageFull => 28,        // ENOSPC
        _ => match raw_os {
            // ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION : fichier tenu par Windows.
            Some(32) | Some(33) => 16, // EBUSY
            Some(145) => 39,           // ERROR_DIR_NOT_EMPTY
            Some(267) => 20,           // ERROR_DIRECTORY
            _ => 5,                    // EIO
        },
    }
}

/// Encode une trame. `data` peut être vide.
pub fn encode_frame<T: Serialize>(msg: &T, data: &[u8]) -> Vec<u8> {
    let json = serde_json::to_vec(msg).unwrap_or_default();
    let mut out = Vec::with_capacity(8 + json.len() + data.len());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    out.extend_from_slice(data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trame_aller_retour() {
        let req = FsRequest::Read {
            path: "c/Users".into(),
            offset: 4096,
            len: 8,
        };
        let f = encode_frame(&req, b"");
        let json_len = u32::from_le_bytes(f[0..4].try_into().unwrap()) as usize;
        let data_len = u32::from_le_bytes(f[4..8].try_into().unwrap()) as usize;
        assert_eq!(data_len, 0);
        let back: FsRequest = serde_json::from_slice(&f[8..8 + json_len]).unwrap();
        assert_eq!(back, req);
        let resp = encode_frame(&FsResponse::Data, b"bonjour");
        assert_eq!(&resp[resp.len() - 7..], b"bonjour");
    }

    #[test]
    fn errno() {
        assert_eq!(errno_for(std::io::ErrorKind::NotFound, None), 2);
        assert_eq!(errno_for(std::io::ErrorKind::Other, Some(32)), 16);
    }
}
