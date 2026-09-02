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
- WSL 2 avec Ubuntu pour construire l'image Linux (`image/build.sh`, en root : `wsl -u root`).
- Une session administrateur pour lancer le service en mode console (`tests/e2e/start-console.ps1`).

Construire :

```powershell
cargo build --workspace --examples
cargo build -p solon-agent --release --target x86_64-unknown-linux-musl
wsl -u root -e bash -c "cd /mnt/c/.../solon && SKIP_KERNEL=1 bash image/build.sh /mnt/c/.../target/x86_64-unknown-linux-musl/release/solon-agent"
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
- **Scripts PowerShell** : toujours enregistrés en UTF-8 **avec BOM** (PowerShell 5.1 lit sinon les accents dans la page de codes ANSI et échoue à l'analyse) ; ne pas nommer une variable `$svc` ou `$root` à côté de `common.ps1` (casse ignorée).
- **Commits atomiques**, format `type(portée): résumé` (`feat(service): …`, `fix(app): …`, `docs: …`).
- **Codes d'erreur** : tout nouvel échec visible par l'utilisateur ajoute une variante à `ErrorCode`,
  un message dans `en.json` et `fr.json`, et une ligne dans le tableau de dépannage du README.
- **Aucune action destructive sans confirmation** dans l'interface ; **aucune requête réseau sortante**
  ajoutée au service ou à l'application.
- **Mesures** : toute affirmation de performance dans la documentation doit venir d'une mesure consignée
  dans `docs/measurements.md`, avec la machine et la date.
- Le vocabulaire de l'interface parle de « moteur » et de « conteneurs », jamais de « machine virtuelle »
  ni de « WSL » ; les journaux techniques, eux, sont explicites.

## Signaler un problème

Joignez : l'édition et le build de Windows (`winver`), le rapport des prérequis (écran de démarrage ou
`solon-service prereq`), le code d'erreur affiché, et `%ProgramData%\Solon\logs\solon-service.log`.
Les lignes préfixées `guest` sont la console de la machine Linux : elles sont précieuses.
