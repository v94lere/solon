#!/usr/bin/env python3
"""Convertit une image disque brute en VHD à taille fixe (format Microsoft VHD, footer « conectix »).

Usage : mkvhd.py <image_brute> <sortie.vhd> [--epoch SECONDES]

Pourquoi un VHD fixe : c'est le format de disque virtuel le plus simple (données brutes + pied de
512 octets), accepté par Hyper-V/HCS, et il se génère sans qemu-img ni outil Windows. Le disque racine
de Solon est en lecture seule et compressé dans l'installeur : la taille « pleine » n'est pas un problème.
Le disque de données, lui, est un VHDX dynamique créé par Windows au premier lancement.
"""
import argparse
import hashlib
import os
import struct
import uuid

SECTOR = 512
MIB = 1024 * 1024
Y2K = 946684800  # 2000-01-01T00:00:00Z, origine des horodatages VHD


def geometry(total_bytes: int):
    """Algorithme CHS de la spécification VHD (appendice)."""
    total_sectors = total_bytes // SECTOR
    if total_sectors > 65535 * 16 * 255:
        total_sectors = 65535 * 16 * 255
    if total_sectors >= 65535 * 16 * 63:
        spt = 255
        heads = 16
        cth = total_sectors // spt
    else:
        spt = 17
        cth = total_sectors // spt
        heads = max((cth + 1023) // 1024, 4)
        if cth >= heads * 1024 or heads > 16:
            spt = 31
            heads = 16
            cth = total_sectors // spt
        if cth >= heads * 1024:
            spt = 63
            heads = 16
            cth = total_sectors // spt
    cylinders = cth // heads
    return cylinders, heads, spt


def footer(size: int, epoch: int, disk_uuid: bytes) -> bytes:
    cyl, heads, spt = geometry(size)
    f = bytearray(512)
    struct.pack_into(">8sIIQI4sII", f, 0,
                     b"conectix",
                     0x00000002,          # features : bit « reserved » toujours à 1
                     0x00010000,          # version du format
                     0xFFFFFFFFFFFFFFFF,  # data offset : aucun (disque fixe)
                     max(0, epoch - Y2K),
                     b"soln",             # application créatrice
                     0x00010000,          # version de l'application
                     0x5769326B)          # hôte créateur « Wi2k »
    struct.pack_into(">QQHBBI", f, 40, size, size, cyl, heads, spt, 2)  # type 2 = fixe
    # Disposition (spécification VHD) : checksum 64-67, UUID 68-83, saved state 84, réservé 85-511.
    f[68:84] = disk_uuid
    f[84] = 0
    checksum = (~sum(f)) & 0xFFFFFFFF  # calculé avec le champ checksum à zéro
    struct.pack_into(">I", f, 64, checksum)
    return bytes(f)


def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("raw")
    p.add_argument("out")
    p.add_argument("--epoch", type=int, default=int(os.environ.get("SOURCE_DATE_EPOCH", "0")))
    a = p.parse_args()

    size = os.path.getsize(a.raw)
    padded = (size + MIB - 1) // MIB * MIB
    # UUID déterministe dérivé du contenu : même image → même disque.
    h = hashlib.sha256()
    with open(a.raw, "rb") as src, open(a.out, "wb") as dst:
        while chunk := src.read(4 * MIB):
            h.update(chunk)
            dst.write(chunk)
        if padded > size:
            dst.write(b"\0" * (padded - size))
        dst.write(footer(padded, a.epoch, uuid.UUID(bytes=h.digest()[:16], version=4).bytes))
    print(f"{a.out}: {padded // MIB} MiB + pied de 512 octets")


if __name__ == "__main__":
    main()
