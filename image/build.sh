#!/usr/bin/env bash
# Pipeline complet de l'image Linux de Solon. À exécuter sous Linux en root (WSL : `wsl -u root`).
#
# Usage : build.sh <binaire solon-agent musl> [dossier_de_sortie]
# Variables : SOLON_IMAGE_VERSION (défaut : 0.1.0-dev.<date>), SKIP_KERNEL=1 pour réutiliser
#             out/kernel/vmlinuz déjà compilé, KERNEL_TAG, ALPINE_BRANCH, SOURCE_DATE_EPOCH.
#
# Produit dans <sortie>/<version>/ : vmlinuz, initrd.img, rootfs.vhd, manifest.json (+ packages.txt,
# kernel.config, kernel.release). Le manifeste porte les SHA-256 et les versions : c'est lui que le
# service Windows vérifie avant de démarrer la machine.
set -euo pipefail

AGENT="${1:?chemin du binaire solon-agent requis}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_BASE="${2:-$HERE/out}"
VERSION="${SOLON_IMAGE_VERSION:-0.1.0-dev.$(date -u +%Y%m%d)}"
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1767225600}"
DEST="$OUT_BASE/$VERSION"
mkdir -p "$DEST"

if [ "${SKIP_KERNEL:-0}" = 1 ] && [ -f "$OUT_BASE/kernel/vmlinuz" ]; then
    echo "==> Noyau réutilisé : $(cat "$OUT_BASE/kernel/kernel.release")"
else
    bash "$HERE/kernel/build-kernel.sh" "$OUT_BASE/kernel"
fi
# rm préalable : un fichier déjà utilisé par le service porte une ACE supplémentaire qui gêne la réécriture depuis WSL.
rm -f "$DEST/vmlinuz" "$DEST/initrd.img" "$DEST/rootfs.vhd" 2>/dev/null || true
cp "$OUT_BASE/kernel/vmlinuz" "$DEST/vmlinuz"
cp "$OUT_BASE/kernel/kernel.release" "$OUT_BASE/kernel/kernel.config" "$DEST/"

bash "$HERE/initrd/build-initrd.sh" "$DEST"
bash "$HERE/rootfs/build-rootfs.sh" "$DEST" "$AGENT"

echo "==> Manifeste"
sha() { sha256sum "$DEST/$1" | cut -d' ' -f1; }
size() { stat -c %s "$DEST/$1"; }
cat > "$DEST/manifest.json" <<EOF
{
  "schema": 1,
  "version": "$VERSION",
  "built_at": "$(date -u -d "@$SOURCE_DATE_EPOCH" +%Y-%m-%dT%H:%M:%SZ)",
  "kernel": { "file": "vmlinuz", "release": "$(cat "$DEST/kernel.release")", "sha256": "$(sha vmlinuz)", "size": $(size vmlinuz) },
  "initrd": { "file": "initrd.img", "sha256": "$(sha initrd.img)", "size": $(size initrd.img) },
  "rootfs": { "file": "rootfs.vhd", "sha256": "$(sha rootfs.vhd)", "size": $(size rootfs.vhd), "alpine_branch": "${ALPINE_BRANCH:-v3.24}" },
  "docker_engine": "$(grep -oE '^docker-engine-[^ ]+' "$DEST/packages.txt" | head -1 | sed 's/docker-engine-//')",
  "kernel_cmdline": "console=ttyS0,115200 8250_core.nr_uarts=1 panic=-1 pci=off rdinit=/init quiet"
}
EOF
rm -f "$DEST"/*.sha256
echo "==> Image $VERSION :"
ls -la "$DEST"
