//! Client FUSE « solonfs » : expose les lecteurs Windows partagés sous `/mnt/solonfs/<lettre>`
//! en interrogeant le serveur de fichiers du service (voir `solon_core::fs`).
//!
//! Ce qui le rend rapide par rapport à 9P : un `Readdir` rapporte les attributs de toutes les
//! entrées et alimente un cache d'attributs (durée courte), `Lookup` renvoie l'attribut complet, et
//! les lectures demandent au moins 128 Ko d'un coup. Les écritures passent directement à l'hôte et
//! invalident le cache de l'entrée. Les droits POSIX n'existent pas côté Windows : tout appartient
//! à root en 0777 (dossiers) / 0666 (fichiers), `chmod`/`chown` sont acceptés et ignorés, comme
//! le fait Docker Desktop.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fuser::{
    FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request,
};
use solon_core::fs::{FS_MAX_IO, FsAttr, FsKind, FsRequest, FsResponse, PORT_FS, encode_frame};

use crate::system::log;
use crate::vsock;

const TTL: Duration = Duration::from_secs(1);
const CACHE_TTL: Duration = Duration::from_millis(1500);
const READAHEAD: u32 = 128 * 1024;
const BLOCK: u32 = 4096;

// ---------------------------------------------------------------------------------------------
// Réserve de connexions (ouvertes par l'hôte) et échange requête / réponse
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Pool {
    free: Mutex<Vec<File>>,
    ready: Condvar,
}

impl Pool {
    fn take(&self) -> File {
        let mut free = self.free.lock().unwrap();
        loop {
            if let Some(f) = free.pop() {
                return f;
            }
            free = self.ready.wait(free).unwrap();
        }
    }
    fn give(&self, f: File) {
        self.free.lock().unwrap().push(f);
        self.ready.notify_one();
    }
}

static POOL: std::sync::OnceLock<Arc<Pool>> = std::sync::OnceLock::new();

fn pool() -> &'static Arc<Pool> {
    POOL.get_or_init(Default::default)
}

/// Accepte les connexions de l'hôte sur le port 5006 et les met dans la réserve.
pub fn serve_pool() {
    let listen_fd = match vsock::listen(PORT_FS) {
        Ok(fd) => fd,
        Err(e) => {
            log(&format!("solonfs : {e}"));
            return;
        }
    };
    log(&format!(
        "solonfs : en attente des connexions de l'hôte (vsock {PORT_FS})"
    ));
    loop {
        match vsock::accept(listen_fd) {
            Ok(f) => pool().give(f),
            Err(e) => {
                log(&format!("solonfs : {e}"));
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

/// Envoie une requête et attend la réponse ; la connexion est rendue à la réserve, ou fermée si elle
/// a échoué (l'hôte en rouvrira une).
fn call(req: &FsRequest, data: &[u8]) -> Result<(FsResponse, Vec<u8>), i32> {
    let mut conn = pool().take();
    let r = (|| -> std::io::Result<(FsResponse, Vec<u8>)> {
        conn.write_all(&encode_frame(req, data))?;
        let mut head = [0u8; 8];
        conn.read_exact(&mut head)?;
        let json_len = u32::from_le_bytes(head[0..4].try_into().unwrap()) as usize;
        let data_len = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
        let mut json = vec![0u8; json_len];
        conn.read_exact(&mut json)?;
        let mut payload = vec![0u8; data_len];
        conn.read_exact(&mut payload)?;
        let resp: FsResponse = serde_json::from_slice(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok((resp, payload))
    })();
    match r {
        Ok(v) => {
            pool().give(conn);
            Ok(v)
        }
        Err(e) => {
            log(&format!("solonfs : connexion perdue : {e}"));
            drop(conn);
            Err(5)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Système de fichiers
// ---------------------------------------------------------------------------------------------

struct Cached<T> {
    value: T,
    at: Instant,
}

struct SolonFs {
    /// Lettre du lecteur (`c`), racine du montage.
    drive: String,
    inodes: HashMap<u64, PathBuf>,
    by_path: HashMap<PathBuf, u64>,
    next_ino: u64,
    attrs: HashMap<u64, Cached<FsAttr>>,
    /// Listes de dossiers : (nom, inode) ; les attributs des enfants sont dans `attrs`.
    dirs: HashMap<u64, Cached<Vec<(String, u64)>>>,
    /// Instantané du listage pris à la première lecture d'un dossier ouvert (par descripteur) : les
    /// lectures suivantes du même descripteur voient la même liste, même si le dossier change entre
    /// deux appels (sinon `rm -rf` saute des entrées et finit sur « dossier non vide »).
    dir_handles: HashMap<u64, Vec<(u64, FileType, String)>>,
    next_fh: u64,
    uid: u32,
    gid: u32,
}

impl SolonFs {
    fn new(drive: &str) -> Self {
        let mut fs = SolonFs {
            drive: drive.to_owned(),
            inodes: HashMap::new(),
            by_path: HashMap::new(),
            next_ino: 2,
            attrs: HashMap::new(),
            dirs: HashMap::new(),
            dir_handles: HashMap::new(),
            next_fh: 1,
            uid: 0,
            gid: 0,
        };
        fs.inodes.insert(1, PathBuf::new());
        fs.by_path.insert(PathBuf::new(), 1);
        fs
    }

    /// Chemin protocole (`c/Users/x`) d'un inode.
    fn rel(&self, ino: u64) -> Option<String> {
        let p = self.inodes.get(&ino)?;
        let mut s = self.drive.clone();
        for c in p.components() {
            s.push('/');
            s.push_str(&c.as_os_str().to_string_lossy());
        }
        Some(s)
    }

    fn ino_for(&mut self, path: PathBuf) -> u64 {
        if let Some(i) = self.by_path.get(&path) {
            return *i;
        }
        let i = self.next_ino;
        self.next_ino += 1;
        self.inodes.insert(i, path.clone());
        self.by_path.insert(path, i);
        i
    }

    fn child_path(&self, parent: u64, name: &OsStr) -> Option<PathBuf> {
        let p = self.inodes.get(&parent)?;
        Some(p.join(name))
    }

    fn remember(&mut self, ino: u64, attr: FsAttr) {
        self.attrs.insert(
            ino,
            Cached {
                value: attr,
                at: Instant::now(),
            },
        );
    }

    fn cached_attr(&self, ino: u64) -> Option<FsAttr> {
        self.attrs
            .get(&ino)
            .filter(|c| c.at.elapsed() < CACHE_TTL)
            .map(|c| c.value.clone())
    }

    fn forget_path(&mut self, ino: u64) {
        self.attrs.remove(&ino);
        self.dirs.remove(&ino);
    }

    fn to_file_attr(&self, ino: u64, a: &FsAttr) -> FileAttr {
        let (kind, perm, nlink) = match a.kind {
            FsKind::Dir => (FileType::Directory, 0o777, 2),
            FsKind::Symlink => (FileType::Symlink, 0o777, 1),
            FsKind::File => (
                FileType::RegularFile,
                if a.readonly { 0o444 } else { 0o666 },
                1,
            ),
        };
        let t = |ns: i64| UNIX_EPOCH + Duration::from_nanos(ns.max(0) as u64);
        FileAttr {
            ino,
            size: a.size,
            blocks: a.size.div_ceil(512),
            atime: t(a.atime_ns),
            mtime: t(a.mtime_ns),
            ctime: t(a.ctime_ns),
            crtime: t(a.ctime_ns),
            kind,
            perm,
            nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK,
            flags: 0,
        }
    }

    fn fetch_attr(&mut self, ino: u64) -> Result<FsAttr, i32> {
        if let Some(a) = self.cached_attr(ino) {
            return Ok(a);
        }
        let path = self.rel(ino).ok_or(2)?;
        match call(&FsRequest::Getattr { path }, &[])? {
            (FsResponse::Attr { attr }, _) => {
                self.remember(ino, attr.clone());
                Ok(attr)
            }
            (FsResponse::Error { errno }, _) => Err(errno),
            _ => Err(5),
        }
    }

    fn fetch_dir(&mut self, ino: u64) -> Result<Vec<(String, u64)>, i32> {
        if let Some(c) = self.dirs.get(&ino) {
            if c.at.elapsed() < CACHE_TTL {
                return Ok(c.value.clone());
            }
        }
        let path = self.rel(ino).ok_or(2)?;
        let dir_path = self.inodes.get(&ino).cloned().ok_or(2)?;
        match call(&FsRequest::Readdir { path }, &[])? {
            (FsResponse::Entries { entries }, _) => {
                let mut list = Vec::with_capacity(entries.len());
                for e in entries {
                    let child = self.ino_for(dir_path.join(&e.name));
                    self.remember(child, e.attr);
                    list.push((e.name, child));
                }
                self.dirs.insert(
                    ino,
                    Cached {
                        value: list.clone(),
                        at: Instant::now(),
                    },
                );
                Ok(list)
            }
            (FsResponse::Error { errno }, _) => Err(errno),
            _ => Err(5),
        }
    }

    fn simple(&mut self, req: FsRequest) -> Result<(), i32> {
        match call(&req, &[])? {
            (FsResponse::Ok, _) => Ok(()),
            (FsResponse::Error { errno }, _) => Err(errno),
            _ => Err(5),
        }
    }
}

fn name_ok(name: &OsStr) -> bool {
    let s = name.to_string_lossy();
    !s.is_empty() && s != "." && s != ".." && !s.contains(['/', '\\'])
}

impl Filesystem for SolonFs {
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        Ok(())
    }

    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        if !name_ok(name) {
            return reply.error(libc::ENOENT);
        }
        let Some(child_path) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        let ino = self.ino_for(child_path);
        if let Some(a) = self.cached_attr(ino) {
            return reply.entry(&TTL, &self.to_file_attr(ino, &a), 0);
        }
        // Si la liste du parent est fraîche et ne contient pas ce nom, il n'existe pas.
        if let Some(c) = self.dirs.get(&parent) {
            if c.at.elapsed() < CACHE_TTL {
                let n = name.to_string_lossy();
                if !c.value.iter().any(|(en, _)| en.eq_ignore_ascii_case(&n)) {
                    return reply.error(libc::ENOENT);
                }
            }
        }
        let Some(parent_rel) = self.rel(parent) else {
            return reply.error(libc::ENOENT);
        };
        match call(
            &FsRequest::Lookup {
                parent: parent_rel,
                name: name.to_string_lossy().into_owned(),
            },
            &[],
        ) {
            Ok((FsResponse::Attr { attr }, _)) => {
                self.remember(ino, attr.clone());
                reply.entry(&TTL, &self.to_file_attr(ino, &attr), 0)
            }
            Ok((FsResponse::Error { errno }, _)) => reply.error(errno),
            Ok(_) => reply.error(libc::EIO),
            Err(e) => reply.error(e),
        }
    }

    fn getattr(&mut self, _req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        match self.fetch_attr(ino) {
            Ok(a) => reply.attr(&TTL, &self.to_file_attr(ino, &a)),
            Err(e) => reply.error(e),
        }
    }

    fn setattr(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        _ctime: Option<SystemTime>,
        _fh: Option<u64>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        if let Some(size) = size {
            if let Err(e) = self.simple(FsRequest::Truncate {
                path: path.clone(),
                size,
            }) {
                return reply.error(e);
            }
        }
        let to_ns = |t: fuser::TimeOrNow| -> i64 {
            let st = match t {
                fuser::TimeOrNow::SpecificTime(s) => s,
                fuser::TimeOrNow::Now => SystemTime::now(),
            };
            st.duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0)
        };
        if atime.is_some() || mtime.is_some() {
            if let Err(e) = self.simple(FsRequest::SetTimes {
                path,
                mtime_ns: mtime.map(to_ns),
                atime_ns: atime.map(to_ns),
            }) {
                return reply.error(e);
            }
        }
        self.forget_path(ino);
        match self.fetch_attr(ino) {
            Ok(a) => reply.attr(&TTL, &self.to_file_attr(ino, &a)),
            Err(e) => reply.error(e),
        }
    }

    fn readlink(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyData) {
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        match call(&FsRequest::Readlink { path }, &[]) {
            Ok((FsResponse::Link { target }, _)) => reply.data(target.as_bytes()),
            Ok((FsResponse::Error { errno }, _)) => reply.error(errno),
            Ok(_) => reply.error(libc::EIO),
            Err(e) => reply.error(e),
        }
    }

    fn mkdir(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        if !name_ok(name) {
            return reply.error(libc::EINVAL);
        }
        let Some(p) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        let ino = self.ino_for(p);
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        match call(&FsRequest::Mkdir { path }, &[]) {
            Ok((FsResponse::Attr { attr }, _)) => {
                self.dirs.remove(&parent);
                self.remember(ino, attr.clone());
                reply.entry(&TTL, &self.to_file_attr(ino, &attr), 0)
            }
            Ok((FsResponse::Error { errno }, _)) => reply.error(errno),
            Ok(_) => reply.error(libc::EIO),
            Err(e) => reply.error(e),
        }
    }

    fn unlink(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let Some(p) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        let ino = self.ino_for(p);
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        match self.simple(FsRequest::Unlink { path }) {
            Ok(()) => {
                self.forget_path(ino);
                self.dirs.remove(&parent);
                reply.ok()
            }
            Err(e) => reply.error(e),
        }
    }

    fn rmdir(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let Some(p) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        let ino = self.ino_for(p);
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        match self.simple(FsRequest::Rmdir { path }) {
            Ok(()) => {
                self.forget_path(ino);
                self.dirs.remove(&parent);
                reply.ok()
            }
            Err(e) => reply.error(e),
        }
    }

    fn rename(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        _flags: u32,
        reply: ReplyEmpty,
    ) {
        if !name_ok(name) || !name_ok(newname) {
            return reply.error(libc::EINVAL);
        }
        let (Some(from_p), Some(to_p)) = (
            self.child_path(parent, name),
            self.child_path(newparent, newname),
        ) else {
            return reply.error(libc::ENOENT);
        };
        let from_ino = self.ino_for(from_p.clone());
        let (Some(from), Some(to)) = (self.rel(from_ino), {
            let i = self.ino_for(to_p.clone());
            self.rel(i)
        }) else {
            return reply.error(libc::ENOENT);
        };
        match self.simple(FsRequest::Rename { from, to }) {
            Ok(()) => {
                // L'inode suit le fichier : on met à jour la table des chemins.
                self.by_path.remove(&from_p);
                if let Some(old) = self.by_path.remove(&to_p) {
                    self.inodes.remove(&old);
                    self.attrs.remove(&old);
                }
                self.inodes.insert(from_ino, to_p.clone());
                self.by_path.insert(to_p, from_ino);
                self.attrs.remove(&from_ino);
                self.dirs.remove(&parent);
                self.dirs.remove(&newparent);
                reply.ok()
            }
            Err(e) => reply.error(e),
        }
    }

    fn open(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
        reply.opened(0, 0);
    }

    fn read(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        let len = size.max(READAHEAD).min(FS_MAX_IO);
        match call(
            &FsRequest::Read {
                path,
                offset: offset.max(0) as u64,
                len,
            },
            &[],
        ) {
            Ok((FsResponse::Data, data)) => {
                let end = (size as usize).min(data.len());
                reply.data(&data[..end])
            }
            Ok((FsResponse::Error { errno }, _)) => reply.error(errno),
            Ok(_) => reply.error(libc::EIO),
            Err(e) => reply.error(e),
        }
    }

    fn write(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        offset: i64,
        data: &[u8],
        _write_flags: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyWrite,
    ) {
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        let mut done = 0usize;
        while done < data.len() {
            let chunk = &data[done..(done + FS_MAX_IO as usize).min(data.len())];
            match call(
                &FsRequest::Write {
                    path: path.clone(),
                    offset: offset.max(0) as u64 + done as u64,
                },
                chunk,
            ) {
                Ok((FsResponse::Written { len }, _)) => done += len as usize,
                Ok((FsResponse::Error { errno }, _)) => return reply.error(errno),
                Ok(_) => return reply.error(libc::EIO),
                Err(e) => return reply.error(e),
            }
        }
        self.forget_path(ino);
        reply.written(done as u32)
    }

    fn flush(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _lock_owner: u64,
        reply: ReplyEmpty,
    ) {
        reply.ok()
    }

    fn release(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        _fh: u64,
        _flags: i32,
        _lock_owner: Option<u64>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        reply.ok()
    }

    fn fsync(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        _fh: u64,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        match self.simple(FsRequest::Fsync { path }) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }

    fn opendir(&mut self, _req: &Request<'_>, _ino: u64, _flags: i32, reply: ReplyOpen) {
        let fh = self.next_fh;
        self.next_fh += 1;
        reply.opened(fh, 0);
    }

    fn readdir(
        &mut self,
        _req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        if offset == 0 || !self.dir_handles.contains_key(&fh) {
            let list = match self.fetch_dir(ino) {
                Ok(l) => l,
                Err(e) => return reply.error(e),
            };
            let parent_ino = self
                .inodes
                .get(&ino)
                .and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
                .and_then(|pp| self.by_path.get(&pp).copied())
                .unwrap_or(1);
            let mut all: Vec<(u64, FileType, String)> = Vec::with_capacity(list.len() + 2);
            all.push((ino, FileType::Directory, ".".into()));
            all.push((parent_ino, FileType::Directory, "..".into()));
            for (name, child) in list {
                let kind = match self.attrs.get(&child).map(|c| c.value.kind) {
                    Some(FsKind::Dir) => FileType::Directory,
                    Some(FsKind::Symlink) => FileType::Symlink,
                    _ => FileType::RegularFile,
                };
                all.push((child, kind, name));
            }
            self.dir_handles.insert(fh, all);
        }
        let Some(all) = self.dir_handles.get(&fh) else {
            return reply.error(libc::EBADF);
        };
        for (i, (child, kind, name)) in all.iter().enumerate().skip(offset.max(0) as usize) {
            if reply.add(*child, (i + 1) as i64, *kind, name) {
                break;
            }
        }
        reply.ok()
    }

    fn releasedir(
        &mut self,
        _req: &Request<'_>,
        _ino: u64,
        fh: u64,
        _flags: i32,
        reply: ReplyEmpty,
    ) {
        self.dir_handles.remove(&fh);
        reply.ok()
    }

    fn statfs(&mut self, _req: &Request<'_>, ino: u64, reply: ReplyStatfs) {
        let path = self.rel(ino).or_else(|| self.rel(1)).unwrap_or_default();
        match call(&FsRequest::Statfs { path }, &[]) {
            Ok((FsResponse::Statfs { statfs }, _)) => {
                let bs = 4096u64;
                reply.statfs(
                    statfs.total_bytes / bs,
                    statfs.free_bytes / bs,
                    statfs.free_bytes / bs,
                    u64::MAX / 2,
                    u64::MAX / 2,
                    bs as u32,
                    255,
                    bs as u32,
                )
            }
            Ok((FsResponse::Error { errno }, _)) => reply.error(errno),
            Ok(_) => reply.error(libc::EIO),
            Err(e) => reply.error(e),
        }
    }

    fn create(
        &mut self,
        _req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        if !name_ok(name) {
            return reply.error(libc::EINVAL);
        }
        let Some(p) = self.child_path(parent, name) else {
            return reply.error(libc::ENOENT);
        };
        let ino = self.ino_for(p);
        let Some(path) = self.rel(ino) else {
            return reply.error(libc::ENOENT);
        };
        match call(&FsRequest::Create { path }, &[]) {
            Ok((FsResponse::Attr { attr }, _)) => {
                self.dirs.remove(&parent);
                self.remember(ino, attr.clone());
                reply.created(&TTL, &self.to_file_attr(ino, &attr), 0, 0, 0)
            }
            Ok((FsResponse::Error { errno }, _)) => reply.error(errno),
            Ok(_) => reply.error(libc::EIO),
            Err(e) => reply.error(e),
        }
    }

    fn access(&mut self, _req: &Request<'_>, _ino: u64, _mask: i32, reply: ReplyEmpty) {
        reply.ok()
    }
}

/// Monte `/mnt/solonfs/<lettre>` dans un thread dédié. Idempotent.
pub fn mount(drive: &str) {
    let target = format!("/mnt/solonfs/{drive}");
    if MOUNTED.lock().unwrap().contains(&target) {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(&target) {
        log(&format!("solonfs : mkdir {target} : {e}"));
        return;
    }
    MOUNTED.lock().unwrap().push(target.clone());
    let drive = drive.to_owned();
    std::thread::spawn(move || {
        let fs = SolonFs::new(&drive);
        let options = [
            MountOption::FSName("solonfs".into()),
            MountOption::Subtype("solonfs".into()),
            MountOption::AllowOther,
            MountOption::DefaultPermissions,
            MountOption::NoAtime,
        ];
        log(&format!("solonfs : montage de {target}"));
        if let Err(e) = fuser::mount2(fs, &target, &options) {
            log(&format!("solonfs : montage {target} : {e}"));
            MOUNTED.lock().unwrap().retain(|t| t != &target);
        }
    });
}

static MOUNTED: Mutex<Vec<String>> = Mutex::new(Vec::new());
