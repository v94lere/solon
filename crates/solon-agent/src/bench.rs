//! Micro-banc d'entrées/sorties (séquentiel 128 MiB, 1 000 petits fichiers) pour `docs/measurements.md`.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};

fn drop_caches() {
    unsafe { libc::sync() };
    let _ = std::fs::write("/proc/sys/vm/drop_caches", "3\n");
}

pub fn run(dir: &str) -> Result<Value, String> {
    let dir = Path::new(dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let big = dir.join("solon-bench.bin");
    let block = vec![0xA5u8; 1024 * 1024];
    let blocks = 128usize;

    let t = Instant::now();
    {
        let mut f = File::create(&big).map_err(|e| format!("create : {e}"))?;
        for _ in 0..blocks {
            f.write_all(&block).map_err(|e| format!("write : {e}"))?;
        }
        f.sync_all().map_err(|e| format!("fsync : {e}"))?;
    }
    let write_s = t.elapsed().as_secs_f64();

    drop_caches();
    let t = Instant::now();
    {
        let mut f = File::open(&big).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 1024 * 1024];
        let mut total = 0usize;
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            total += n;
        }
        if total != blocks * 1024 * 1024 {
            return Err(format!("lecture incomplète : {total}"));
        }
    }
    let read_s = t.elapsed().as_secs_f64();
    let _ = std::fs::remove_file(&big);

    let small_dir = dir.join("solon-bench-small");
    let _ = std::fs::remove_dir_all(&small_dir);
    std::fs::create_dir_all(&small_dir).map_err(|e| e.to_string())?;
    let n_small = 1000usize;
    let payload = vec![0x5Au8; 4096];

    let t = Instant::now();
    for i in 0..n_small {
        let mut f =
            File::create(small_dir.join(format!("f{i:04}.dat"))).map_err(|e| e.to_string())?;
        f.write_all(&payload).map_err(|e| e.to_string())?;
    }
    let create_ms = t.elapsed().as_millis() as u64;

    drop_caches();
    let t = Instant::now();
    for i in 0..n_small {
        std::fs::metadata(small_dir.join(format!("f{i:04}.dat"))).map_err(|e| e.to_string())?;
    }
    let stat_ms = t.elapsed().as_millis() as u64;

    let t = Instant::now();
    let mut buf = Vec::with_capacity(4096);
    for i in 0..n_small {
        buf.clear();
        File::open(small_dir.join(format!("f{i:04}.dat")))
            .map_err(|e| e.to_string())?
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
    }
    let read_small_ms = t.elapsed().as_millis() as u64;

    let t = Instant::now();
    let entries = std::fs::read_dir(&small_dir)
        .map_err(|e| e.to_string())?
        .count();
    let readdir_ms = t.elapsed().as_millis() as u64;

    let t = Instant::now();
    std::fs::remove_dir_all(&small_dir).map_err(|e| e.to_string())?;
    let delete_ms = t.elapsed().as_millis() as u64;

    Ok(json!({
        "seq_write_mib_s": (blocks as f64 / write_s).round(),
        "seq_read_mib_s": (blocks as f64 / read_s).round(),
        "small_files": n_small,
        "small_create_ms": create_ms,
        "small_stat_ms": stat_ms,
        "small_read_ms": read_small_ms,
        "readdir_ms": readdir_ms,
        "readdir_entries": entries,
        "small_delete_ms": delete_ms,
    }))
}
