#!/usr/bin/env python3
"""Construit un initramfs (cpio newc + gzip) reproductible sans dépendre de cpio.

Usage : mkinitramfs.py <dossier_source> <sortie.gz>
Modes : 0755 pour `init`, tout ce qui est sous bin/ ou sbin/, et tout fichier portant un bit x ;
0644 sinon. Aucun uid/gid, aucune date (SOURCE_DATE_EPOCH ignoré : les entrées sont à 0).
"""
import gzip
import os
import stat
import sys


def newc_header(name: bytes, mode: int, size: int, ino: int, nlink: int = 1) -> bytes:
    fields = [ino, mode, 0, 0, nlink, 0, size, 0, 0, 0, 0, len(name) + 1, 0]
    return b"070701" + b"".join(f"{f:08x}".encode() for f in fields) + name + b"\0"


def pad4(data: bytearray) -> None:
    while len(data) % 4:
        data += b"\0"


def build(src: str, out: str) -> None:
    archive = bytearray()
    ino = 1
    entries = []
    for root, dirs, files in os.walk(src):
        dirs.sort()
        rel_root = os.path.relpath(root, src)
        if rel_root != ".":
            entries.append((rel_root.replace(os.sep, "/"), None))
        for f in sorted(files):
            full = os.path.join(root, f)
            entries.append((os.path.relpath(full, src).replace(os.sep, "/"), full))
    for rel, full in entries:
        name = rel.encode()
        if full is None:
            archive += newc_header(name, stat.S_IFDIR | 0o755, 0, ino, 2)
            pad4(archive)
        else:
            with open(full, "rb") as fh:
                data = fh.read()
            if rel == "init":
                data = data.replace(b"\r\n", b"\n")
            st = os.stat(full)
            executable = rel == "init" or rel.startswith(("bin/", "sbin/")) or bool(st.st_mode & 0o111)
            mode = stat.S_IFREG | (0o755 if executable else 0o644)
            archive += newc_header(name, mode, len(data), ino)
            pad4(archive)
            archive += data
            pad4(archive)
        ino += 1
    archive += newc_header(b"TRAILER!!!", 0, 0, ino)
    pad4(archive)
    with open(out, "wb") as fh:
        fh.write(gzip.compress(bytes(archive), compresslevel=9, mtime=0))
    print(f"{out}: {os.path.getsize(out)} octets, {len(entries)} entrées")


if __name__ == "__main__":
    build(sys.argv[1], sys.argv[2])
