//! Serveur de fichiers « solonfs » : répond aux requêtes du client FUSE de l'invité sur les dossiers
//! Windows des lecteurs partagés. Voir `solon_core::fs` pour le protocole.
//!
//! Le service ouvre [`FS_CONNECTIONS`] connexions vers l'invité (port vsock 5006) et sert chacune
//! dans sa propre tâche ; les opérations disque se font dans des threads bloquants.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use solon_core::fs::{
    FS_CONNECTIONS, FS_MAX_IO, FsAttr, FsEntry, FsKind, FsRequest, FsResponse, FsStatfs, PORT_FS,
    encode_frame, errno_for,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use windows::core::GUID;

use crate::engine::Engine;

/// Préchauffage : la **première** ouverture d'un fichier côté Windows coûte ~3,5 ms (NTFS, antivirus),
/// contre ~0,2 ms ensuite. Quand un dossier est listé, ses petits fichiers sont lus une fois en tâche
/// de fond pour que la première lecture depuis un conteneur soit déjà rapide (`find … | xargs cat`,
/// `npm install`, `git status`). Borné : au plus 64 fichiers de 256 Ko par dossier, 4 lectures en
/// parallèle, jamais pour les gros fichiers.
const PREFETCH_MAX_FILES: usize = 64;
const PREFETCH_MAX_SIZE: u64 = 256 * 1024;
static PREFETCH_SLOTS: std::sync::LazyLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(tokio::sync::Semaphore::new(4)));

fn prefetch(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    for p in paths {
        let slots = PREFETCH_SLOTS.clone();
        handle.spawn(async move {
            let Ok(_permit) = slots.acquire().await else {
                return;
            };
            let _ = tokio::task::spawn_blocking(move || {
                use std::io::Read;
                if let Ok(mut f) = std::fs::File::open(&p) {
                    let mut sink = [0u8; 64 * 1024];
                    while matches!(f.read(&mut sink), Ok(n) if n > 0) {}
                }
            })
            .await;
        });
    }
}

pub async fn serve(engine: Engine, vm_id: GUID) {
    for i in 0..FS_CONNECTIONS {
        let engine = engine.clone();
        tokio::spawn(async move {
            // L'agent écoute dès son démarrage ; on réessaie si la connexion tombe.
            loop {
                match connect(vm_id).await {
                    Ok(stream) => {
                        if let Err(e) = serve_connection(&engine, stream).await {
                            tracing::debug!(i, "solonfs : connexion terminée : {e}");
                        }
                    }
                    Err(e) => {
                        tracing::debug!(i, "solonfs : connexion impossible : {e}");
                    }
                }
                if engine.is_stopped().await {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }
}

async fn connect(vm_id: GUID) -> io::Result<tokio::net::TcpStream> {
    let std_stream = tokio::task::spawn_blocking(move || {
        solon_hvsock::connect_with_retry(&vm_id, PORT_FS, Duration::from_secs(5))
    })
    .await
    .map_err(io::Error::other)??;
    std_stream.set_nonblocking(true)?;
    tokio::net::TcpStream::from_std(std_stream)
}

async fn serve_connection(engine: &Engine, mut stream: tokio::net::TcpStream) -> io::Result<()> {
    loop {
        let mut head = [0u8; 8];
        stream.read_exact(&mut head).await?;
        let json_len = u32::from_le_bytes(head[0..4].try_into().unwrap()) as usize;
        let data_len = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
        if json_len > 1 << 20 || data_len > FS_MAX_IO as usize + 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "trame trop grande",
            ));
        }
        let mut json = vec![0u8; json_len];
        stream.read_exact(&mut json).await?;
        let mut data = vec![0u8; data_len];
        stream.read_exact(&mut data).await?;
        let req: FsRequest = match serde_json::from_slice(&json) {
            Ok(r) => r,
            Err(_) => {
                stream
                    .write_all(&encode_frame(&FsResponse::Error { errno: 22 }, &[]))
                    .await?;
                continue;
            }
        };
        let shared = engine.shared_drives().await;
        let (resp, payload) = tokio::task::spawn_blocking(move || handle(&shared, req, data))
            .await
            .unwrap_or((FsResponse::Error { errno: 5 }, Vec::new()));
        stream.write_all(&encode_frame(&resp, &payload)).await?;
    }
}

/// `c/Users/v` → `C:\Users\v` si le lecteur `c` est partagé.
fn host_path(shared: &[String], rel: &str) -> Result<PathBuf, i32> {
    let rel = rel.trim_matches('/');
    let (drive, rest) = rel.split_once('/').unwrap_or((rel, ""));
    if drive.len() != 1 || !drive.as_bytes()[0].is_ascii_alphabetic() {
        return Err(2);
    }
    let drive = drive.to_ascii_lowercase();
    if !shared.iter().any(|d| d.eq_ignore_ascii_case(&drive)) {
        return Err(13);
    }
    // Refus des segments spéciaux (déjà résolus par le noyau côté invité, mais on ne prend pas de risque).
    if rest.split('/').any(|s| s == ".." || s == ".") {
        return Err(22);
    }
    let mut p = PathBuf::from(format!("{}:\\", drive.to_ascii_uppercase()));
    if !rest.is_empty() {
        p.push(rest.replace('/', "\\"));
    }
    Ok(p)
}

fn ns(t: io::Result<SystemTime>) -> i64 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

fn attr_of(md: &std::fs::Metadata) -> FsAttr {
    let kind = if md.is_dir() {
        FsKind::Dir
    } else if md.file_type().is_symlink() {
        FsKind::Symlink
    } else {
        FsKind::File
    };
    FsAttr {
        kind,
        size: if md.is_dir() { 0 } else { md.len() },
        mtime_ns: ns(md.modified()),
        atime_ns: ns(md.accessed()),
        ctime_ns: ns(md.created()),
        readonly: md.permissions().readonly(),
    }
}

fn err(e: io::Error) -> FsResponse {
    FsResponse::Error {
        errno: errno_for(e.kind(), e.raw_os_error()),
    }
}

fn handle(shared: &[String], req: FsRequest, data: Vec<u8>) -> (FsResponse, Vec<u8>) {
    let hp = |rel: &str| host_path(shared, rel);
    let r = match req {
        FsRequest::Lookup { parent, name } => {
            if name.contains(['/', '\\']) || name == ".." || name == "." {
                return (FsResponse::Error { errno: 22 }, vec![]);
            }
            match hp(&parent) {
                Ok(p) => match std::fs::symlink_metadata(p.join(&name)) {
                    Ok(md) => FsResponse::Attr { attr: attr_of(&md) },
                    Err(e) => err(e),
                },
                Err(errno) => FsResponse::Error { errno },
            }
        }
        FsRequest::Getattr { path } => match hp(&path) {
            Ok(p) => match std::fs::symlink_metadata(&p) {
                Ok(md) => FsResponse::Attr { attr: attr_of(&md) },
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Readdir { path } => match hp(&path) {
            Ok(p) => match std::fs::read_dir(&p) {
                Ok(rd) => {
                    let mut entries = Vec::new();
                    let mut warm: Vec<PathBuf> = Vec::new();
                    for e in rd.flatten() {
                        // Sous Windows, `DirEntry::metadata` vient du listage : pas d'appel par fichier.
                        if let Ok(md) = e.metadata() {
                            if md.is_file()
                                && md.len() <= PREFETCH_MAX_SIZE
                                && warm.len() < PREFETCH_MAX_FILES
                            {
                                warm.push(e.path());
                            }
                            entries.push(FsEntry {
                                name: e.file_name().to_string_lossy().into_owned(),
                                attr: attr_of(&md),
                            });
                        }
                    }
                    prefetch(warm);
                    FsResponse::Entries { entries }
                }
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Read { path, offset, len } => match hp(&path) {
            Ok(p) => match read_at(&p, offset, len.min(FS_MAX_IO)) {
                Ok(bytes) => return (FsResponse::Data, bytes),
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Write { path, offset } => match hp(&path) {
            Ok(p) => match write_at(&p, offset, &data) {
                Ok(n) => FsResponse::Written { len: n },
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Create { path } => match hp(&path) {
            Ok(p) => match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&p)
                .and_then(|_| std::fs::symlink_metadata(&p))
            {
                Ok(md) => FsResponse::Attr { attr: attr_of(&md) },
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Mkdir { path } => match hp(&path) {
            Ok(p) => match std::fs::create_dir(&p).and_then(|_| std::fs::symlink_metadata(&p)) {
                Ok(md) => FsResponse::Attr { attr: attr_of(&md) },
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Unlink { path } => match hp(&path) {
            Ok(p) => match std::fs::remove_file(&p) {
                Ok(()) => FsResponse::Ok,
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Rmdir { path } => match hp(&path) {
            Ok(p) => match std::fs::remove_dir(&p) {
                Ok(()) => FsResponse::Ok,
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Rename { from, to } => match (hp(&from), hp(&to)) {
            (Ok(a), Ok(b)) => {
                // Sémantique POSIX : écraser la cible si c'est un fichier.
                if b.is_file() {
                    let _ = std::fs::remove_file(&b);
                }
                match std::fs::rename(&a, &b) {
                    Ok(()) => FsResponse::Ok,
                    Err(e) => err(e),
                }
            }
            (Err(errno), _) | (_, Err(errno)) => FsResponse::Error { errno },
        },
        FsRequest::Truncate { path, size } => match hp(&path) {
            Ok(p) => match std::fs::OpenOptions::new()
                .write(true)
                .open(&p)
                .and_then(|f| f.set_len(size))
            {
                Ok(()) => FsResponse::Ok,
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::SetTimes {
            path,
            mtime_ns,
            atime_ns,
        } => match hp(&path) {
            Ok(p) => {
                let f = std::fs::OpenOptions::new().write(true).open(&p);
                match f {
                    Ok(f) => {
                        let mut times = std::fs::FileTimes::new();
                        if let Some(m) = mtime_ns {
                            times = times
                                .set_modified(UNIX_EPOCH + Duration::from_nanos(m.max(0) as u64));
                        }
                        if let Some(a) = atime_ns {
                            times = times
                                .set_accessed(UNIX_EPOCH + Duration::from_nanos(a.max(0) as u64));
                        }
                        match f.set_times(times) {
                            Ok(()) => FsResponse::Ok,
                            Err(e) => err(e),
                        }
                    }
                    // Dossier ou fichier en lecture seule : on ignore silencieusement (comme Docker Desktop).
                    Err(_) => FsResponse::Ok,
                }
            }
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Fsync { path } => match hp(&path) {
            Ok(p) => match std::fs::OpenOptions::new()
                .write(true)
                .open(&p)
                .and_then(|f| f.sync_all())
            {
                Ok(()) => FsResponse::Ok,
                Err(_) => FsResponse::Ok,
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Statfs { path } => match hp(&path) {
            Ok(p) => match disk_space(&p) {
                Some((total, free)) => FsResponse::Statfs {
                    statfs: FsStatfs {
                        total_bytes: total,
                        free_bytes: free,
                    },
                },
                None => FsResponse::Error { errno: 5 },
            },
            Err(errno) => FsResponse::Error { errno },
        },
        FsRequest::Readlink { path } => match hp(&path) {
            Ok(p) => match std::fs::read_link(&p) {
                Ok(t) => FsResponse::Link {
                    target: t.to_string_lossy().replace('\\', "/"),
                },
                Err(e) => err(e),
            },
            Err(errno) => FsResponse::Error { errno },
        },
    };
    (r, vec![])
}

fn read_at(p: &Path, offset: u64, len: u32) -> io::Result<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(p)?;
    let size = f.metadata()?.len();
    if offset >= size {
        return Ok(Vec::new());
    }
    let want = (len as u64).min(size - offset) as usize;
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = vec![0u8; want];
    let mut filled = 0;
    while filled < want {
        let n = f.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

fn write_at(p: &Path, offset: u64, data: &[u8]) -> io::Result<u32> {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().write(true).open(p)?;
    f.seek(SeekFrom::Start(offset))?;
    f.write_all(data)?;
    Ok(data.len() as u32)
}

fn disk_space(p: &Path) -> Option<(u64, u64)> {
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows::core::HSTRING;
    let root = p.ancestors().last()?.to_path_buf();
    let mut free = 0u64;
    let mut total = 0u64;
    let mut total_free = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(
            &HSTRING::from(root.as_os_str()),
            Some(&mut free),
            Some(&mut total),
            Some(&mut total_free),
        )
    }
    .ok()?;
    Some((total, free))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins() {
        let shared = vec!["c".to_string()];
        assert_eq!(
            host_path(&shared, "c/Users/v").unwrap(),
            PathBuf::from(r"C:\Users\v")
        );
        assert_eq!(host_path(&shared, "c").unwrap(), PathBuf::from(r"C:\"));
        assert_eq!(host_path(&shared, "d/x"), Err(13));
        assert_eq!(host_path(&shared, "c/../x"), Err(22));
        assert_eq!(host_path(&shared, "zz/x"), Err(2));
    }
}
