# Contribuer à Solon

Merci de votre intérêt. Solon est un projet Rust/TypeScript qui touche à la virtualisation Windows :
les contributions les plus utiles sont des rapports précis, des tests sur d'autres configurations
(éditions de Windows, antivirus, VPN) et des correctifs ciblés.

## Organisation du dépôt

| Dossier | Rôle |
|---|---|
| `crates/solon-core` | Types partagés : erreurs à codes stables, protocole hôte↔agent, protocole app↔service |
| `crates/solon-vm-hcs` | Pilotage de la machine via l'API Host Compute System (création, ACL, partages, événements) |
| `crates/solon-hvsock` | Sockets Hyper-V côté hôte, relais named pipe → HvSocket |
| `crates/solon-prereq` | Détection des prérequis Windows |
| `crates/solon-service` | Service Windows : machine à états, réseau HNS, disque, relais de ports, canal de contrôle |
| `crates/solon-agent` | PID 1 de la machine Linux (musl statique) : montages, dockerd, RPC, événements |
| `apps/desktop` | Application Tauri v2 (React, TypeScript, Tailwind), anglais par défaut, français |
| `image/` | Pipeline reproductible de l'image Linux : noyau, système racine Alpine, initrd, manifeste |
| `tests/e2e` | Scénarios de bout en bout contre le vrai service |
| `docs/` | Mesures et faits établis |

Lisez `ARCHITECTURE.md` avant de toucher au provisionnement : chaque section « Résultats du bloc »
liste les pièges déjà rencontrés (ACL du groupe Virtual Machines, `HVSOCKET_CONNECT_TIMEOUT`,
périphérique Plan9 à déclarer dès la création, `docker events` qui ne vide pas sa sortie, etc.).

## Mettre en place un poste de développement

- Windows 11 Pro/Entreprise/Éducation avec Hyper-V et la Plateforme de machine virtuelle activés.
- Rust stable (`rustup`), cible `x86_64-unknown-linux-musl` (`rustup target add x86_64-unknown-linux-musl`).
- Node 22 et npm.
- Solon installé : l'image Linux se construit dans un conteneur Solon (`image/build.sh` sous Alpine, ~15 s), plus besoin de WSL.
- Une session administrateur pour lancer le service en mode console (`tests/e2e/start-console.ps1`).

Construire :

```powershell
cargo build --workspace --examples
cargo build -p solon-agent --release --target x86_64-unknown-linux-musl
docker run --rm -v "${PWD}:/work" -w /work public.ecr.aws/docker/library/alpine:3.24 sh -c "apk add -q bash curl python3 e2fsprogs coreutils tar grep findutils gzip; SKIP_KERNEL=1 SOLON_IMAGE_VERSION=0.1.0-dev.N bash image/build.sh /work/target/x86_64-unknown-linux-musl/release/solon-agent"
cd apps\desktop; npm install; npm run typecheck
```

Le noyau se compile une fois (`image/kernel/build-kernel.sh`, ~7 min sur 16 cœurs) et se réutilise
avec `SKIP_KERNEL=1`.

## Tests

- `cargo test --workspace` : tests unitaires (schéma HCS, HRESULT, protocole, chemins, couverture des
  traductions).
- `cargo test -p solon-vm-hcs --test boot` avec `SOLON_IT=1` : tests d'intégration réels (Administrateur,
  voir l'en-tête du fichier).
- `tests/e2e/*.ps1` : scénarios complets (service console, `docker` sans élévation, port publié,
  coupure brutale, crash du service). Détails dans `tests/e2e/README.md`.

Toute modification du provisionnement, du réseau ou de l'agent doit passer `tests/e2e/e2e.ps1` et
`crash-force-stop.ps1` avant d'être proposée.

## Règles

- **Langue** : code et identifiants en anglais ; commentaires, commits, documentation en français ;
  interface en anglais par défaut avec traduction française complète (le test `locales.rs` vérifie la parité).
- **Scripts PowerShell** : toujours enregistrés en UTF-8 **avec BOM** (PowerShell 5.1 lit sinon les accents dans la page de codes ANSI et échoue à l'analyse) ; ne pas nommer une variable `$svc` ou `$root` à côté de `common.ps1` (casse ignorée). Dans une chaîne, un nom de variable suivi de deux-points s'écrit `${nom}:` (sinon PowerShell lit `$nom:` comme un lecteur et **tout le script échoue à l'analyse**, sans journal : l'installeur laisse alors le service arrêté). Avant de livrer : `[System.Management.Automation.Language.Parser]::ParseFile("installer\setup.ps1", [ref]$null, [ref]$e)` doit laisser `$e` vide.
- **Commits atomiques**, format `type(portée): résumé` (`feat(service): …`, `fix(app): …`, `docs: …`).
- **Codes d'erreur** : tout nouvel échec visible par l'utilisateur ajoute une variante à `ErrorCode`,
  un message dans `en.json` et `fr.json`, et une ligne dans le tableau de dépannage du README.
- **Aucune action destructive sans confirmation** dans l'interface ; **aucune requête réseau sortante**
  ajoutée au service ou à l'application.
- **Mesures** : toute affirmation de performance dans la documentation doit venir d'une mesure consignée
  dans `docs/measurements.md`, avec la machine et la date.
- Le vocabulaire de l'interface parle de « moteur » et de « conteneurs », jamais de « machine virtuelle »
  ni de « WSL » ; les journaux techniques, eux, sont explicites.

## Accepter les contributions : Developer Certificate of Origin

Pas de contrat de cession de droits : chaque commit porte une ligne `Signed-off-by: Prénom Nom <e-mail>`
(`git commit -s`), qui atteste que vous avez le droit de contribuer ce code sous la licence du projet
(Apache-2.0), selon le [Developer Certificate of Origin 1.1](https://developercertificate.org/). Les
contributions sans cette ligne ne sont pas fusionnées.

Le projet suit le code de conduite du `CODE_OF_CONDUCT.md`. Les failles de sécurité se signalent en privé,
voir `SECURITY.md`.

### Licences des dépendances

`THIRD-PARTY.md` liste tout ce que l'installeur et l'image redistribuent. Quand une dépendance change :
`cargo metadata --format-version 1` donne les licences des crates, `apps/desktop/node_modules/*/package.json`
celles des paquets npm, et `image/out/<version>/packages.txt` les paquets Alpine de l'image. Toute nouvelle
dépendance sous licence copyleft forte (GPL, AGPL) doit être discutée avant d'être ajoutée.

## Signaler un problème

Joignez : l'édition et le build de Windows (`winver`), le rapport des prérequis (écran de démarrage ou
`solon-service prereq`), le code d'erreur affiché, et `%ProgramData%\Solon\logs\solon-service.log`.
Les lignes préfixées `guest` sont la console de la machine Linux : elles sont précieuses.
