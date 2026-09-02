#!/usr/bin/env bash
# Construit le système racine de Solon : Alpine Linux minimal + Docker Engine + agent Solon,
# dans une image ext4 en lecture seule convertie en VHD fixe.
#
# Usage (Linux root, ex. `wsl -u root`) : build-rootfs.sh <dossier_de_sortie> <binaire solon-agent>
# Variables : ALPINE_BRANCH (défaut v3.24), ALPINE_MIRROR, ROOTFS_WORKDIR (disque ext4), SOURCE_DATE_EPOCH.
# Sortie : rootfs.vhd, rootfs.sha256, packages.txt (versions exactes installées)
#
# Reproductibilité : la branche Alpine est épinglée et les versions installées sont consignées dans
# packages.txt. Les miroirs Alpine ne conservant pas les anciennes versions d'une branche, une
# reconstruction bit-à-bit exige un miroir figé : prévu en phase 2, documenté dans ARCHITECTURE.md.
set -euo pipefail

OUT="${1:?dossier de sortie requis}"
AGENT="${2:?chemin du binaire solon-agent (x86_64 musl statique) requis}"
ALPINE_BRANCH="${ALPINE_BRANCH:-v3.24}"
ALPINE_MIRROR="${ALPINE_MIRROR:-https://dl-cdn.alpinelinux.org/alpine}"
ROOTFS_WORKDIR="${ROOTFS_WORKDIR:-/root/solon-build/rootfs}"
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1767225600}" # 2026-01-01T00:00:00Z
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS="$HERE/../tools"

[ "$(id -u)" = 0 ] || { echo "à exécuter en root (propriétaires des fichiers du système racine)" >&2; exit 1; }
[ -f "$AGENT" ] || { echo "agent introuvable : $AGENT" >&2; exit 1; }

PACKAGES=(
    # base
    busybox musl alpine-baselayout alpine-keys ca-certificates-bundle tzdata
    # moteur de conteneurs
    docker-engine docker-cli docker-cli-compose containerd runc tini-static
    # réseau des conteneurs (dockerd programme netfilter via iptables-nft)
    iptables ip6tables
    # disques de données : formatage et vérification d'ext4 au démarrage
    e2fsprogs e2fsprogs-extra
    # diagnostic léger
    util-linux-misc
)

MAIN="$ALPINE_MIRROR/$ALPINE_BRANCH/main"
COMMUNITY="$ALPINE_MIRROR/$ALPINE_BRANCH/community"
WORK="$ROOTFS_WORKDIR"
ROOT="$WORK/root"
mkdir -p "$WORK/dl" "$WORK/keys" "$OUT"
rm -rf "$ROOT"
mkdir -p "$ROOT"

fetch_apk() { # <repo_url> <nom_paquet> -> chemin local
    local repo="$1" name="$2" file
    file="$(curl -fsSL "$repo/x86_64/" | grep -oE "$name-[0-9][^\"<]*\.apk" | head -1)"
    [ -n "$file" ] || { echo "paquet $name introuvable dans $repo/x86_64" >&2; exit 1; }
    [ -f "$WORK/dl/$file" ] || curl -fsSL -o "$WORK/dl/$file" "$repo/x86_64/$file"
    echo "$WORK/dl/$file"
}

echo "==> apk-tools-static et clés Alpine ($ALPINE_BRANCH)"
APK_TOOLS="$(fetch_apk "$MAIN" apk-tools-static)"
tar -xzf "$APK_TOOLS" -C "$WORK" sbin/apk.static 2>/dev/null
KEYS_APK="$(fetch_apk "$MAIN" alpine-keys)"
tar -xzf "$KEYS_APK" -C "$WORK/keys" 2>/dev/null || true
APK="$WORK/sbin/apk.static"

echo "==> Installation des paquets"
"$APK" --root "$ROOT" --arch x86_64 --keys-dir "$WORK/keys/etc/apk/keys" \
    --repository "$MAIN" --repository "$COMMUNITY" \
    --initdb --no-cache --no-progress add "${PACKAGES[@]}"
printf '%s\n%s\n' "$MAIN" "$COMMUNITY" > "$ROOT/etc/apk/repositories"
"$APK" --root "$ROOT" info -v | sort > "$OUT/packages.txt"

echo "==> Agent Solon et configuration"
install -m 0755 "$AGENT" "$ROOT/sbin/solon-agent"
install -d "$ROOT/etc/docker" "$ROOT/etc/solon" "$ROOT/var/lib/docker" "$ROOT/var/lib/containerd" \
           "$ROOT/var/lib/solon" "$ROOT/mnt/host" "$ROOT/run"
install -m 0644 "$HERE/files/daemon.json" "$ROOT/etc/docker/daemon.json"
install -m 0644 "$HERE/files/containerd.toml" "$ROOT/etc/containerd/config.toml" 2>/dev/null || {
    install -d "$ROOT/etc/containerd"; install -m 0644 "$HERE/files/containerd.toml" "$ROOT/etc/containerd/config.toml"; }
echo "solon" > "$ROOT/etc/hostname"
printf 'nameserver 1.1.1.1\nnameserver 8.8.8.8\n' > "$ROOT/etc/resolv.conf"  # remplacé par l'agent au démarrage
echo "$ALPINE_BRANCH" > "$ROOT/etc/solon/alpine-branch"
date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%dT%H:%M:%SZ > "$ROOT/etc/solon/build-date"

echo "==> Nettoyage"
rm -rf "$ROOT/var/cache/apk"/* "$ROOT/usr/share/man" "$ROOT/usr/share/doc" "$ROOT/usr/share/info" \
       "$ROOT/usr/share/bash-completion" "$ROOT/usr/share/fish" "$ROOT/usr/share/zsh" \
       "$ROOT/etc/init.d" "$ROOT/etc/conf.d" "$ROOT/etc/runlevels" "$ROOT/etc/periodic" "$ROOT/etc/crontabs"
find "$ROOT" -depth -type d -empty -path '*/usr/share/*' -delete 2>/dev/null || true
# Horodatage uniforme pour une image déterministe.
find "$ROOT" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +

SIZE_MB=$(( $(du -sm "$ROOT" | cut -f1) * 115 / 100 + 24 ))
echo "==> Image ext4 (${SIZE_MB} Mio, lecture seule, sans journal)"
IMG="$WORK/rootfs.img"
rm -f "$IMG"
E2FSPROGS_FAKE_TIME="$SOURCE_DATE_EPOCH" mkfs.ext4 -q -F -d "$ROOT" -L solon-root \
    -O ^has_journal,^huge_file -m 0 -E root_owner=0:0,hash_seed=00000000-0000-0000-0000-000000000000 \
    -U 5ec7a0a0-0000-4000-8000-000000000001 "$IMG" "${SIZE_MB}M"
e2fsck -fn "$IMG" >/dev/null

echo "==> VHD fixe"
python3 "$TOOLS/mkvhd.py" "$IMG" "$OUT/rootfs.vhd"
(cd "$OUT" && sha256sum rootfs.vhd > rootfs.sha256)
echo "==> Système racine : $(du -h "$OUT/rootfs.vhd" | cut -f1), $(wc -l < "$OUT/packages.txt") paquets"
