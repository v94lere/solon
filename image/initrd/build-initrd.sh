#!/usr/bin/env bash
# Construit l'initramfs minimal de Solon : busybox statique (paquet Alpine) + le script init.
# Usage (Linux) : build-initrd.sh <dossier_de_sortie>
# Variables : ALPINE_BRANCH, ALPINE_MIRROR, INITRD_WORKDIR
# Sortie : initrd.img, initrd.sha256
set -euo pipefail

OUT="${1:?dossier de sortie requis}"
ALPINE_BRANCH="${ALPINE_BRANCH:-v3.24}"
ALPINE_MIRROR="${ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"
INITRD_WORKDIR="${INITRD_WORKDIR:-/root/solon-build/initrd}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS="$HERE/../tools"
MAIN="$ALPINE_MIRROR/$ALPINE_BRANCH/main"

mkdir -p "$INITRD_WORKDIR/dl" "$OUT"
TREE="$INITRD_WORKDIR/tree"
rm -rf "$TREE"
mkdir -p "$TREE/bin" "$TREE/proc" "$TREE/sys" "$TREE/dev" "$TREE/run" "$TREE/newroot"

# Page téléchargée d'abord : avec `pipefail`, `head -1` fermerait le tube avant la fin de curl (SIGPIPE) et
# ferait échouer le script sous busybox/Alpine.
page="$(curl -fsSL "$MAIN/x86_64/")"
file="$(printf '%s' "$page" | grep -oE 'busybox-static-[0-9][^"<]*\.apk' | head -1 || true)"
[ -n "$file" ] || { echo "busybox-static introuvable" >&2; exit 1; }
[ -f "$INITRD_WORKDIR/dl/$file" ] || curl -fsSL -o "$INITRD_WORKDIR/dl/$file" "$MAIN/x86_64/$file"
tar -xzf "$INITRD_WORKDIR/dl/$file" -C "$INITRD_WORKDIR" bin/busybox.static 2>/dev/null
install -m 0755 "$INITRD_WORKDIR/bin/busybox.static" "$TREE/bin/busybox"
sed 's/\r$//' "$HERE/init" > "$TREE/init"
chmod 0755 "$TREE/init"

python3 "$TOOLS/mkinitramfs.py" "$TREE" "$OUT/initrd.img"
(cd "$OUT" && sha256sum initrd.img > initrd.sha256)
echo "==> initrd : $(du -h "$OUT/initrd.img" | cut -f1) (busybox $file)"
