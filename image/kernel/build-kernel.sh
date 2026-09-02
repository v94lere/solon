#!/usr/bin/env bash
# Compile le noyau Solon à partir d'un tag du dépôt microsoft/WSL2-Linux-Kernel.
#
# Usage (Linux, ex. WSL Ubuntu) : build-kernel.sh <dossier_de_sortie>
# Variables : KERNEL_TAG (défaut ci-dessous), KERNEL_WORKDIR (sources et objets, sur un disque ext4,
#             pas sous /mnt/c : la compilation y serait très lente), JOBS.
# Sortie : vmlinuz, kernel.release, kernel.config, kernel.sha256
#
# Pourquoi ce dépôt : c'est la configuration que Microsoft maintient pour les machines HCS
# (Hyper-V, hv_sock, 9P, ballon mémoire), vérifiée au bloc 0a. On la reprend telle quelle et on
# applique le fragment solon.config (voir ce fichier pour la liste des changements).
set -euo pipefail

OUT="${1:?dossier de sortie requis}"
KERNEL_TAG="${KERNEL_TAG:-linux-msft-wsl-6.18.40.1}"
KERNEL_WORKDIR="${KERNEL_WORKDIR:-/root/solon-build/kernel}"
JOBS="${JOBS:-$(nproc)}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FRAGMENT="$HERE/solon.config"

# Reproductibilité : horodatage, utilisateur et hôte fixes dans la bannière du noyau.
export KBUILD_BUILD_TIMESTAMP="${KBUILD_BUILD_TIMESTAMP:-2026-01-01T00:00:00Z}"
export KBUILD_BUILD_USER=solon
export KBUILD_BUILD_HOST=build
export KCFLAGS="${KCFLAGS:--fdebug-prefix-map=$KERNEL_WORKDIR=.}"

mkdir -p "$KERNEL_WORKDIR" "$OUT"
cd "$KERNEL_WORKDIR"

if [ ! -d "$KERNEL_TAG" ]; then
    echo "==> Téléchargement des sources $KERNEL_TAG"
    curl -fL --retry 3 -o "$KERNEL_TAG.tar.gz" \
        "https://github.com/microsoft/WSL2-Linux-Kernel/archive/refs/tags/$KERNEL_TAG.tar.gz"
    mkdir "$KERNEL_TAG"
    tar -xzf "$KERNEL_TAG.tar.gz" -C "$KERNEL_TAG" --strip-components=1
    rm -f "$KERNEL_TAG.tar.gz"
fi
cd "$KERNEL_TAG"

echo "==> Configuration : Microsoft/config-wsl + $FRAGMENT"
cp Microsoft/config-wsl .config
# merge_config.sh applique le fragment ; -m évite un olddefconfig implicite, fait explicitement ensuite.
KCONFIG_CONFIG=.config scripts/kconfig/merge_config.sh -m .config "$FRAGMENT" >/dev/null
# Solon n'embarque aucun module : tout ce que Docker peut demander à netfilter / au réseau doit être
# en dur. On convertit en =y les familles réseau que la config WSL laisse en modules (constaté au
# bloc 1 : « Extension addrtype revision 0 not supported » avec NETFILTER_XT_MATCH_ADDRTYPE=m).
sed -i -E 's/^(CONFIG_(NF_|NFT_|NETFILTER_|IP_NF_|IP6_NF_|IP_SET|IP_VS|BRIDGE_|NET_SCH_|NET_CLS_|NET_ACT_|DUMMY|MACVLAN|IPVLAN|VXLAN|GENEVE|TUN|VETH|XFRM_|INET_DIAG|NETLINK_DIAG|UNIX_DIAG|PACKET_DIAG|TCP_CONG_|INET_TUNNEL|IPV6_)[A-Z0-9_]*)=m$/\1=y/' .config
make olddefconfig >/dev/null

# Vérifie que les options critiques sont bien en dur.
for opt in HYPERV HYPERV_VSOCKETS HYPERV_STORAGE HYPERV_NET HYPERV_UTILS HYPERV_BALLOON PAGE_REPORTING \
           NET_9P 9P_FS 9P_FS_POSIX_ACL EXT4_FS OVERLAY_FS SQUASHFS EROFS_FS BRIDGE VETH VSOCKETS \
           NF_TABLES CGROUPS SERIAL_8250 SERIAL_8250_CONSOLE IKCONFIG_PROC \
           NETFILTER_XT_MATCH_ADDRTYPE NETFILTER_XT_MATCH_CONNTRACK NETFILTER_XT_TARGET_MASQUERADE \
           NETFILTER_XT_NAT NETFILTER_XT_MARK IP_NF_IPTABLES IP_NF_NAT IP_NF_FILTER IP6_NF_IPTABLES \
           NFT_COMPAT NFT_NAT NFT_MASQ NFT_REJECT NF_CONNTRACK NF_NAT BRIDGE_NETFILTER; do
    grep -q "^CONFIG_${opt}=y" .config || { echo "ERREUR : CONFIG_${opt} n'est pas =y" >&2; exit 1; }
done

echo "==> Compilation (-j$JOBS)"
make -j"$JOBS" bzImage 2>&1 | tail -3

cp arch/x86/boot/bzImage "$OUT/vmlinuz"
make -s kernelrelease > "$OUT/kernel.release"
cp .config "$OUT/kernel.config"
(cd "$OUT" && sha256sum vmlinuz > kernel.sha256)
echo "==> Noyau : $(cat "$OUT/kernel.release"), $(du -h "$OUT/vmlinuz" | cut -f1)"
