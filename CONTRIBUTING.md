# Contribuer à Solon

Merci de votre intérêt. Solon est un projet Rust qui touche à la virtualisation Windows :
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
| `crates/solon-agent` | PID 1 de la machine Linux (musl statique) : montages, dockerd, RPC, événements, client solonfs, lecture des sondes eBPF |
| `crates/solon-docker-shim` | Le `docker.exe` livré dans `bin\` : lance le CLI Docker officiel en le dirigeant vers le moteur Solon, sauf hôte ou contexte explicite |
| `crates/solon-client` | Côté client du service, sans interface : canal de contrôle, détection Compose, `docker ps` partagé, sauvegarde et restauration d'un projet, copie de volumes, import depuis Docker Desktop, mise à jour (seul endroit où vivent reqwest et bollard), export de diagnostic. Partagé par l'icône et le CLI : une fonction qui sert aux deux va ici, pas dans `src-tauri` |
| `crates/solon-cli` | La commande `solon` (`bin\solon.exe` une fois installée ; `solon-cli.exe` dans `target/`, l'icône s'appelant déjà `solon.exe`). La voie principale pour les nouvelles fonctions depuis le 31 octobre 2026 |
| `crates/solon-trace-common` | Vocabulaire `no_std` partagé entre les sondes eBPF et l'agent (disposition mémoire des événements) |
| `crates/solon-trace-ebpf` | Sondes eBPF de la Radiographie (hors espace de travail Cargo : cible `bpfel-unknown-none`, compilées dans un conteneur, voir son `Dockerfile`) |
| `apps/desktop` | L'icône de la barre des tâches : programme Tauri v2 sans fenêtre ni frontend (`src-tauri/src` : `main.rs`, `lib.rs`, `service.rs`, `tray.rs`, `lang.rs`). `ui/index.html` vide exigé par l'empaqueteur ; `package.json` ne garde que `@tauri-apps/cli`. L'application de bureau a été retirée en 0.2.0 (31 octobre 2026) |
| `image/` | Pipeline reproductible de l'image Linux : noyau, système racine Alpine, initrd, manifeste |
| `installer/` | Scripts de l'installeur NSIS : activation des composants Windows, service, PATH, signature |
| `packaging/` | Paquet MSIX pour le Microsoft Store et manifeste winget |
| `site/` | Site de présentation (Astro, statique), publié sur GitHub Pages par le workflow « Site » |
| `bench/` | Script de comparaison avec Docker Desktop et sa pile Compose |
| `tests/e2e` | Scénarios de bout en bout contre le vrai service |
| `docs/` | Guide fonctionnel, Radiographie, mesures et faits établis |

Lisez `ARCHITECTURE.md` avant de toucher au provisionnement : chaque section « Résultats du bloc »
liste les pièges déjà rencontrés (ACL du groupe Virtual Machines, `HVSOCKET_CONNECT_TIMEOUT`,
périphérique Plan9 à déclarer dès la création, `docker events` qui ne vide pas sa sortie, etc.).

## Mettre en place un poste de développement

- Windows 11 ou 10 22H2, toutes éditions, avec Hyper-V et la Plateforme de machine virtuelle activés (sur
  Famille, la Plateforme de machine virtuelle seule ; le repli 9P du partage de fichiers n'y existe pas).
- Rust stable (`rustup`), cible `x86_64-unknown-linux-musl` (`rustup target add x86_64-unknown-linux-musl`).
- Node 22 et npm, seulement pour l'empaqueteur Tauri (`@tauri-apps/cli`) : il n'y a plus de JavaScript dans Solon.
- Solon installé : l'image Linux se construit dans un conteneur Solon (`image/build.sh` sous Alpine, ~15 s), plus besoin de WSL.
- Une session administrateur pour lancer le service en mode console (`tests/e2e/start-console.ps1`).

Construire :

```powershell
cargo build --workspace --examples
cargo build -p solon-agent --release --target x86_64-unknown-linux-musl
docker run --rm -v "${PWD}:/work" -w /work public.ecr.aws/docker/library/alpine:3.24 sh -c "apk add -q bash curl python3 e2fsprogs coreutils tar grep findutils gzip; SKIP_KERNEL=1 SOLON_IMAGE_VERSION=0.1.0-dev.N bash image/build.sh /work/target/x86_64-unknown-linux-musl/release/solon-agent"
cd apps\desktop; npm install
```

Le noyau se compile une fois (`image/kernel/build-kernel.sh`, ~7 min sur 16 cœurs) et se réutilise
avec `SKIP_KERNEL=1`.

Pour produire un installeur, l'ordre compte :

```powershell
cargo build --release -p solon-service -p solon-cli
cd apps\desktop; npm run tauri build
```

Le service et le CLI sont copiés comme ressources (`tauri.conf.json` : `target/release/solon-service.exe` et
`target/release/solon-cli.exe` → `bin/solon.exe`) : `tauri build` ne les reconstruit pas, il embarque ce qui
est dans `target/release` au moment où il passe. Un `tauri build` seul livre donc un service et un `solon`
périmés sans rien dire.

## Tests

- `cargo test --workspace` : tests unitaires (schéma HCS, HRESULT, protocole, chemins, libellés et icônes de
  la barre des tâches).
- `cargo test -p solon-vm-hcs --test boot` avec `SOLON_IT=1` : tests d'intégration réels (Administrateur,
  voir l'en-tête du fichier).
- `tests/e2e/*.ps1` : scénarios complets (service console, `docker` sans élévation, port publié,
  coupure brutale, crash du service). Détails dans `tests/e2e/README.md`.

Toute modification du provisionnement, du réseau ou de l'agent doit passer `tests/e2e/e2e.ps1` et
`crash-force-stop.ps1` avant d'être proposée.

## Règles

- **Langue** : code et identifiants en anglais ; commentaires, commits, documentation en français ;
  textes visibles en anglais par défaut, en français quand Windows l'est. Il n'y a pas de fichiers de
  traduction (ils ont disparu avec la fenêtre, le 31 octobre 2026) : tout texte rédigé par le CLI ou l'icône
  s'écrit dans les deux langues à l'endroit où il sert, par `tr("English text", "Texte français")`
  (`crates/solon-cli/src/text.rs` pour la commande, `apps/desktop/src-tauri/src/lang.rs` pour l'icône) ; un
  texte anglais seul n'est pas accepté. Exception, par convention : les messages d'erreur du crate
  `solon-client` sont techniques et en anglais seul ; le CLI les préfixe d'un « error: » / « erreur : »
  traduit. Les identifiants de commandes, d'options et de réglages (`status`, `--json`, `memory=`) restent en
  anglais dans les deux langues.
- **Scripts PowerShell** : toujours enregistrés en UTF-8 **avec BOM** (PowerShell 5.1 lit sinon les accents dans la page de codes ANSI et échoue à l'analyse) ; ne pas nommer une variable `$svc` ou `$root` à côté de `common.ps1` (casse ignorée). Dans une chaîne, un nom de variable suivi de deux-points s'écrit `${nom}:` (sinon PowerShell lit `$nom:` comme un lecteur et **tout le script échoue à l'analyse**, sans journal : l'installeur laisse alors le service arrêté). Avant de livrer : `[System.Management.Automation.Language.Parser]::ParseFile("installer\setup.ps1", [ref]$null, [ref]$e)` doit laisser `$e` vide.
- **Commits atomiques**, format `type(portée): résumé` (`feat(service): …`, `fix(app): …`, `docs: …`).
- **Codes d'erreur** : tout nouvel échec visible par l'utilisateur ajoute une variante à `ErrorCode` et une
  ligne dans le tableau de dépannage des deux README (`README.md`, `README.fr.md`). Si le CLI ou l'icône doit
  l'expliquer, un texte `tr(en, fr)` ; sinon le CLI affiche `[CODE] message` tel que le service l'envoie.
- **Aucune action destructive sans confirmation** ni option explicite (`--force`) ; **aucune requête réseau
  sortante** ajoutée au service, à l'icône ou au CLI : la seule qui existe est `solon update`, lancée par
  l'utilisateur.
- **Mesures** : toute affirmation de performance dans la documentation doit venir d'une mesure consignée
  dans `docs/measurements.md`, avec la machine et la date.
- Le vocabulaire des textes visibles (CLI, icône, README) parle de « moteur » et de « conteneurs », jamais de
  « machine virtuelle » ni de « WSL » ; les journaux techniques, eux, sont explicites.

## Accepter les contributions : Developer Certificate of Origin

Pas de contrat de cession de droits : chaque commit porte une ligne `Signed-off-by: Prénom Nom <e-mail>`
(`git commit -s`), qui atteste que vous avez le droit de contribuer ce code sous la licence du projet
(Apache-2.0), selon le [Developer Certificate of Origin 1.1](https://developercertificate.org/). Les
contributions sans cette ligne ne sont pas fusionnées.

Le projet suit le code de conduite du `CODE_OF_CONDUCT.md`. Les failles de sécurité se signalent en privé,
voir `SECURITY.md`.

### Licences des dépendances

`THIRD-PARTY.md` liste tout ce que l'installeur et l'image redistribuent. Quand une dépendance change :
`cargo metadata --format-version 1` donne les licences des crates et `image/out/<version>/packages.txt` les
paquets Alpine de l'image (le seul paquet npm, `@tauri-apps/cli`, sert à construire et n'est pas redistribué). Toute nouvelle
dépendance sous licence copyleft forte (GPL, AGPL) doit être discutée avant d'être ajoutée.

## Signaler un problème

Joignez : le zip de `solon diag`, ou à défaut l'édition et le build de Windows (`winver`), le rapport des
prérequis (`solon prereq`), le code d'erreur affiché par `solon status`, et
`%ProgramData%\Solon\logs\solon-service.log`.
Les lignes préfixées `guest` sont la console de la machine Linux : elles sont précieuses.
