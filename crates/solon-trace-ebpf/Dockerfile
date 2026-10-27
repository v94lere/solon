# Atelier de compilation des programmes eBPF de Solon.
#
# Pourquoi un conteneur alors que tout le reste de Solon se compile depuis Windows : bpf-linker,
# l'éditeur de liens qui transforme le bitcode produit par rustc en objet BPF, ne se construit pas
# sous Windows — son script de construction appelle `std::os::unix::ffi::OsStrExt`. C'est la seule
# pièce de Solon qui a besoin d'un hôte Linux, et c'est Solon lui-même qui la fabrique.
#
# Le couple Rust / LLVM est figé, et il ne se choisit pas librement : bpf-linker relit le bitcode
# émis par rustc, sa LLVM doit donc être au moins aussi récente. Or apt.llvm.org ne publie pas de
# llvm-23 (vérifié sur bookworm, trixie, noble et plucky : 20, 22 et 24 seulement), et la version
# 0.12 de bpf-linker ne connaît pas encore LLVM 24. nightly-2026-07-01 utilise LLVM 22.1.8 : c'est
# le couple retenu. Le relever demandera d'attendre que bpf-linker gère LLVM 24.
FROM debian:trixie-slim

ARG RUST_NIGHTLY=nightly-2026-07-01
ARG LLVM_VERSION=22

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates curl gnupg build-essential \
        zlib1g-dev libzstd-dev libffi-dev libxml2-dev \
    && curl -fsSL https://apt.llvm.org/llvm-snapshot.gpg.key \
        | gpg --dearmor > /usr/share/keyrings/llvm.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/llvm.gpg] http://apt.llvm.org/trixie/ llvm-toolchain-trixie main" \
        > /etc/apt/sources.list.d/llvm.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        "llvm-${LLVM_VERSION}-dev" "libpolly-${LLVM_VERSION}-dev" \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain "${RUST_NIGHTLY}" --component rust-src

# bpf-linker localise LLVM par `llvm-config`, que le paquet Debian installe hors du PATH.
ENV LLVM_PREFIX=/usr/lib/llvm-22 PATH=/usr/lib/llvm-22/bin:/usr/local/cargo/bin:$PATH
RUN cargo install bpf-linker --version 0.12.0 --locked \
        --no-default-features --features "llvm-${LLVM_VERSION}"

COPY build-in-container.sh /usr/local/bin/build-ebpf
RUN chmod +x /usr/local/bin/build-ebpf

WORKDIR /src
ENTRYPOINT ["/usr/local/bin/build-ebpf"]
