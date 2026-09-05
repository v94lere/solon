# fuser 0.15.1 (copie locale)

Copie de la crate `fuser` 0.15.1 (licence MIT, https://github.com/cberner/fuser) avec une seule modification :
`build.rs` refusait de compiler sans libfuse quand la **machine hôte** n'est pas Linux, alors que seule la
**cible** compte (nous compilons l'agent depuis Windows vers `x86_64-unknown-linux-musl`). Le test a été retiré.
Déclarée via `[patch.crates-io]` dans le `Cargo.toml` de l'espace de travail. À supprimer quand la version amont
utilisera `CARGO_CFG_TARGET_OS`.
