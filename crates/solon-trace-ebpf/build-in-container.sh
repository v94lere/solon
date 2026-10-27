#!/usr/bin/env bash
# Compile les programmes eBPF et dépose l'objet là où l'agent l'attend.
#
# Lancé par `build.ps1`, avec le dépôt monté en /src. Le dossier de compilation reste dans le
# conteneur : écrire des centaines de fichiers intermédiaires à travers le partage Windows serait
# lent, et `cargo` y buterait sur les permissions.
set -euo pipefail

DEST="/src/crates/solon-agent/src/bpf/solon-trace.o"

cd /src/crates/solon-trace-ebpf
export CARGO_TARGET_DIR=/tmp/target
cargo build --release

OBJ=/tmp/target/bpfel-unknown-none/release/solon-trace
if [ ! -f "$OBJ" ]; then
    echo "objet BPF introuvable : $OBJ" >&2
    exit 1
fi

mkdir -p "$(dirname "$DEST")"
cp "$OBJ" "$DEST"
echo "objet BPF : $(stat -c %s "$DEST") octets, sha256 $(sha256sum "$DEST" | cut -d' ' -f1)"
echo "sections :"
llvm-objdump-22 --section-headers "$DEST" | sed -n '5,$p'
