# Solon

*English version: [README.md](README.md).*

[![CI](https://github.com/v94lere/solon/actions/workflows/ci.yml/badge.svg)](https://github.com/v94lere/solon/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/v94lere/solon?label=release&color=f860c0)](https://github.com/v94lere/solon/releases/latest) [![Downloads](https://img.shields.io/github/downloads/v94lere/solon/total?color=e01818)](https://github.com/v94lere/solon/releases) [![License](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE) [![Website](https://img.shields.io/badge/website-v94lere.github.io%2Fsolon-1a0d14)](https://v94lere.github.io/solon/)

![Nouvelle pile : n8n créé et démarré, puis ouvert sur https://n8n.n8n.solon.local](docs/media/new-stack.gif)

*Une pile en un clic : New stack… → n8n → Create and start → le navigateur s'ouvre sur son adresse https://…solon.local.*


**Solon est un gestionnaire de conteneurs autonome pour Windows**, dans l'esprit d'OrbStack : une
installation, aucun prérequis logiciel à installer soi-même, et un moteur Docker complet qui démarre
en quelques secondes dans une machine minuscule et invisible, gérée entièrement par Solon.

Solon **n'est pas** une interface pour un Docker Desktop déjà installé. Il remplace Docker Desktop :
moteur Docker, Compose, images, volumes, réseaux, terminal, journaux, et une icône dans la barre des tâches.

> État du projet (8 septembre 2026) : **0.1.0, bêta**. Tout ce qui suit fonctionne sur la machine de
> développement (Windows 11 Pro) et sur une machine vierge sans Docker Desktop ni WSL. L'installeur n'est
> **pas encore signé** (avertissement SmartScreen). Retours et rapports de bug bienvenus.

## Ce que Solon fait

- Démarre un moteur Docker (dockerd, containerd, runc, Compose) prêt en **~2,5 s** après l'ordre de démarrage.
- Expose l'API Docker sur `\\.\pipe\solon` : l'interface Solon et le CLI `docker` que vous avez déjà
  fonctionnent (`docker -H npipe:////./pipe/solon ps`, ou un contexte `docker context create`).
- Relaie les ports publiés vers `localhost` sans configuration.
- Partage vos dossiers Windows à la demande : `docker run -v C:\...`, `--mount`, projets Compose, exactement
  comme avec Docker Desktop (le lecteur est partagé à la volée et remonté à chaque démarrage).
- Mémoire au repos (moteur + service) : **430 à 520 Mo** mesurés pour 2 Go alloués.
- **Aucune télémétrie, aucune requête réseau sortante** en dehors de ce que vos conteneurs et vos
  `docker pull` demandent.

## Comparatif : Docker Desktop et Solon

Même machine (Windows 11 Pro, 24 cœurs logiques, NVMe), même pile Compose (une application web Python et sa base PostgreSQL 16,
`bench/compose.yaml`), chaque moteur avec ses réglages par défaut : Docker Desktop 4.66.1 sur WSL2 avec tous les
cœurs et 6,6 Go visibles par les conteneurs ; Solon 0.1.0 avec 22 processeurs et 2 Go. Docker Desktop a été
mesuré le 3 septembre 2026, puis désinstallé de la machine de test ; Solon a été remesuré le 10 septembre 2026
avec le même script. Détails et chiffres bruts : `docs/measurements.md`.

| | Docker Desktop | Solon | |
|---|---|---|---|
| Moteur prêt après l'ordre de démarrage | 6,1 s | **2,6 à 3,4 s** (1,1 s si la machine tourne encore) | 2× plus rapide |
| Mémoire au repos, aucun conteneur | 1 998 Mo | **430 à 520 Mo** | 4× plus léger |
| Mémoire avec l'application web + PostgreSQL, au repos | 5 146 Mo | **1 196 Mo** | 4× plus léger |
| `compose down` puis `up -d`, images présentes | 7,1 s | **5,2 s** | |
| L'application web répond après `up` | 1,5 s | 1,0 à 2,5 s | équivalent |
| Page de connexion de l'application, moyenne de 5 chargements | 27 à 46 ms | 48 à 88 ms | équivalent |
| Initialisation de la base de l'application avec données d'exemple (CPU, surtout mono-thread) | 13,6 s | 12,9 à 13,4 s | équivalent |
| Écritures synchrones sur un volume Docker (`pg_test_fsync`, fdatasync / fsync) | 154 / 79 ops/s | **240 à 290 / 132 à 137 ops/s** | 1,7× plus rapide |
| Dossier Windows monté dans un conteneur, 5 000 fichiers (listage / attributs / lectures / écritures) | 9P | **solonfs** : 6,6× / 94× / 3,6 à 9× / 4× plus rapide | |
| `docker run --rm busybox true`, à chaud | non mesuré | 0,55 s | |

Ce que cela dit, honnêtement : Solon gagne nettement sur ce qui coûte tous les jours (démarrage, mémoire,
partage de fichiers, écritures disque) ; sur le calcul pur les deux moteurs sont équivalents ; la latence web est
identique. Les deux gardent le cache disque de l'invité en mémoire : Solon est plafonné par son allocation (2 Go
par défaut), Docker Desktop est monté à 5 Go.

Pour reproduire : `powershell -ExecutionPolicy Bypass -File bench\bench.ps1` sur le moteur visé par votre
commande `docker` (`-Docker "C:\Program Files\Solon\bin\docker.exe"` pour Solon). Le script ne touche qu'un
projet Compose nommé `solon-bench` sur le port 18069 et le supprime à la fin.

## Prérequis

| Prérequis | Détail |
|---|---|
| Windows | **Windows 11 (ou Windows 10 22H2) Pro, Entreprise ou Éducation**. Windows Famille n'est pas pris en charge dans cette version : il lui manque un composant Hyper-V nécessaire au partage de fichiers (voir `ARCHITECTURE.md` §13, risque R2). |
| Processeur | Virtualisation matérielle **activée dans le BIOS/UEFI** : Intel VT-x (souvent « Intel Virtualization Technology ») ou AMD-V (« SVM Mode »). |
| Mémoire | 8 Go recommandés (4 Go minimum). Solon alloue 2 Go au moteur par défaut, ajustable dans les réglages. |
| Disque | ~400 Mo pour Solon et son image Linux, plus un disque de données dynamique (64 Go maximum par défaut, occupé à la demande). |
| Composants Windows | « Hyper-V » et « Plateforme de machine virtuelle ». L'installeur les active ; un redémarrage peut être demandé. |

Solon cohabite avec WSL2 et Docker Desktop : il n'utilise ni leur pipe, ni leurs réseaux, ni leur
contexte `docker` par défaut.

## Installation

### Installeur

`Solon_0.1.0_x64-setup.exe` (80 Mo, produit par `npm run tauri build` dans `apps/desktop`, sorti dans
`target/release/bundle/nsis/`). Il demande l'élévation une fois, puis : active les composants Windows
nécessaires (un redémarrage peut être demandé), installe le service `SolonService`, copie l'image Linux et
crée le raccourci. Installation silencieuse : `Solon_0.1.0_x64-setup.exe /S`.

La version de développement **n'est pas signée** : Windows SmartScreen affichera « Windows a protégé votre
ordinateur » ; cliquez « Informations complémentaires » puis « Exécuter quand même ». La chaîne de signature
est prête (`installer/sign.ps1`, `tauri.signed.conf.json`) et s'active dès qu'un certificat est disponible.

Désinstallation : Paramètres → Applications → Solon. Le désinstalleur arrête le moteur, retire le service et
**demande** avant de supprimer vos images et volumes (`%ProgramData%\Solon`) ; par défaut ils sont conservés.

### Depuis les sources (développeurs)

Prérequis : Rust stable (≥ 1.85), Node 22, et **Solon lui-même** installé (l'image Linux du moteur se construit
dans un conteneur Solon : pas besoin de WSL). Une session Windows **administrateur** sert seulement au mode console.

```powershell
# 1. Agent invité (compilation croisée depuis Windows, sans chaîne C)
rustup target add x86_64-unknown-linux-musl
cargo build -p solon-agent --release --target x86_64-unknown-linux-musl

# 2. Image Linux (noyau réutilisé + système racine Alpine + initrd), construite dans un conteneur Solon (~15 s)
docker run --rm -v "${PWD}:/work" -w /work public.ecr.aws/docker/library/alpine:3.24 sh -c "apk add -q bash curl python3 e2fsprogs coreutils tar grep findutils gzip; SKIP_KERNEL=1 SOLON_IMAGE_VERSION=0.1.0-dev.N bash image/build.sh /work/target/x86_64-unknown-linux-musl/release/solon-agent"
#    (le noyau lui-même se compile une fois avec image/kernel/build-kernel.sh, ~7 min, dans le même genre de conteneur)

# 3. Service et application (l'installeur embarque image/out/<version>, voir tauri.conf.json)
cargo build --release -p solon-service
cd apps\desktop && npm install && npm run tauri build
```

Pour le développement, le service se lance en mode console (fenêtre UAC) et l'application en mode dev :

```powershell
.\tests\e2e\start-console.ps1 -ImageDir .\image\out\<version> -Release
cd apps\desktop; npm run tauri dev
```

## Utilisation

Le bloc en bas du menu montre l'état du moteur, son temps de fonctionnement et deux mini-jauges (processeur, mémoire).
L'icône de la barre des tâches porte un point vert, orange, rouge ou gris selon l'état. La taille et la position de la
fenêtre sont mémorisées.

Le menu de gauche a deux parties : **Docker** (Containers, Volumes, Images, Networks) et **General** (Activity,
Terminal, Settings). Le bouton en haut du menu (ou `Ctrl+B`) le replie en icônes seules ; le choix est mémorisé.

- **Conteneurs** : liste temps réel avec CPU et mémoire, filtre, groupes Compose, actions en icônes, journaux en
  flux, terminal, inspection. Un **port publié est un lien** vers `http://localhost:<port>`, le domaine local aussi.
  Les **projets Compose** vivent ici : « Ouvrir un projet… » choisit un dossier contenant `compose.yaml`, la liste
  déroulante rappelle les projets récents, et un clic sur l'en-tête d'un groupe Compose ouvre l'écran du projet :
  services et leur état, **journaux de tous les services mêlés**, Up / Down / Reconstruire avec la **sortie en
  direct**, ouverture du dossier dans l'**Explorateur** ou dans **VS Code**.
- **Réveil à la demande** : un conteneur joint via Solon (adresse `solon.local` ou port publié) qui ne reçoit plus
  rien pendant dix minutes (réglable) est **mis en pause** : zéro processeur, mémoire conservée. La première requête
  suivante le réveille en une fraction de seconde, avant d'être servie. Les bases de données et les tâches de fond
  qui ne parlent qu'en interne ne sont jamais concernées ; un bouton « Keep awake » dans la fiche exclut un
  conteneur ; l'état « Asleep » apparaît dans la liste.
- **Piles prêtes et détection de projet** : « New stack… » ouvre une galerie : des kits de
  démarrage qui génèrent un projet au premier lancement (Django + PostgreSQL, Flask + Redis, FastAPI +
  PostgreSQL, Next.js), des applications prêtes (WordPress, PostgreSQL, MariaDB, MongoDB, Redis, n8n, Nextcloud,
  Ghost, Gitea, Uptime Kuma, Jupyter Lab, site statique Nginx) et des outils de développement (Mailpit) : on
  choisit, on désigne un dossier, on relit le `compose.yaml` généré, « Créer et démarrer ». Ouvrir un dossier sans fichier
  Compose fait regarder à Solon ce qu'il contient (package.json, requirements.txt, Dockerfile, composer.json,
  go.mod, Cargo.toml, pom.xml, .csproj, Gemfile…) et proposer un environnement, modifiable
  avant création. Rien n'est jamais écrasé.
- **Fiche d'un conteneur** (clic sur son nom) : onglet **Overview** avec image, commande, dates, politique de
  redémarrage, réseaux (adresse, passerelle, alias), ports, montages, variables d'environnement, étiquettes, et
  **copie de fichiers** dans les deux sens (`docker cp` sans ligne de commande). Onglets Logs, Terminal, Inspect.
- **Fichiers** (onglet de la fiche d'un conteneur, et bouton « Files » ou clic sur le nom d'un volume) : un
  explorateur des fichiers du conteneur ou du volume : parcourir, copier vers Windows, envoyer des fichiers ou un
  dossier (bouton ou **glisser-déposer** depuis l'Explorateur), créer un dossier, supprimer. Fonctionne sans shell
  dans l'image ; les conteneurs doivent être en marche pour être parcourus, les volumes toujours.
- **Debug shell** (onglet de la fiche) : un shell d'outils (bash, curl, dig, ps, strace, tcpdump, jq, vim…) qui
  partage les processus, le réseau et les volumes du conteneur **même si son image n'a aucun shell** (images
  « distroless », Go, .NET). Le système de fichiers du conteneur est visible sous `/proc/1/root`. Rien n'est
  modifié dans le conteneur ; la boîte à outils (`solon-debug`) se construit au premier usage.
- **Images, Volumes, Réseaux** : liste, création, inspection, suppression (toujours avec confirmation).
- **Des adresses qui marchent, toujours** : chaque conteneur en marche est joignable sur
  `https://<nom>.solon.local` (et `https://<service>.<projet>.solon.local` pour Compose), **qu'il publie un port
  ou non**. Solon lit le port que l'image expose (80, 8080, 3000, 8069…), tient à jour un bloc dans le fichier
  `hosts` de Windows, et route les requêtes depuis `127.0.0.1:80` (HTTP) et `127.0.0.1:443` (HTTPS) d'après le nom
  demandé. Le HTTPS repose sur une autorité de certification locale « Solon Local CA », créée à la première
  utilisation et installée dans le magasin racine de la machine : le navigateur n'affiche aucun avertissement.
  Les **adresses des conteneurs** (`10.90.x.y`) sont aussi joignables directement depuis Windows (`curl
  http://10.90.0.2/`, un client PostgreSQL vers `10.90.1.2:5432`…). Si un autre logiciel occupe le port 80 ou
  443, la partie correspondante est désactivée et les ports publiés restent utilisables. `curl.exe` de Windows
  refuse les certificats sans point de révocation : ajouter `--ssl-no-revoke` (comme avec mkcert).
- **Notifications Windows** : conteneur arrêté avec une erreur (hors actions faites dans Solon), moteur en échec
  ou en redémarrage, disque du moteur plein à 90 %.
- **Diagnostic** (Réglages → Exporter un diagnostic) : une archive zip avec les journaux, l'état, les réglages,
  les prérequis et `docker info`, à joindre à un rapport de bug. Aucun identifiant n'y figure.
- **Activity** : performances en temps réel. En haut, le moteur : processeur, mémoire (utilisée / totale, réservée),
  stockage des données, réseau, avec les courbes de la dernière minute. En dessous, chaque conteneur en marche :
  processeur, mémoire, débit réseau, courbe du processeur ; colonnes triables. Rien n'est installé dans les conteneurs.
- **Terminal** (section, ou `Ctrl+\``) : un shell root dans le moteur Linux lui-même, pour `docker`, `ps`, `df`,
  `dmesg`… La session reste ouverte quand on change de section ; « Nouvelle session » en relance une.
- **Recherche globale `Ctrl+K`** : conteneurs, images, volumes, réseaux, projets, actions du moteur, sections.
  `Ctrl+1` à `Ctrl+7` changent de section dans l'ordre du menu.
- **Réglages** : langue (anglais par défaut, français), apparence (clair, sombre, Windows), couleur d'accent (bleu
  Solon ou couleur de Windows), mémoire et processeurs du moteur (par défaut
  tous les cœurs moins deux), limite de stockage, démarrage à l'ouverture de session.
- **Barre des tâches** : un clic sur l'icône ouvre le menu : état du moteur, nombre de conteneurs en
  marche, **chaque conteneur avec Démarrer / Redémarrer / Arrêter**, ouvrir Solon, démarrer ou arrêter le
  moteur, quitter. Dans la langue de l'interface. Fermer la fenêtre laisse Solon actif dans la barre des tâches.

### CLI `docker` et `docker compose` intégrés

L'installeur place `C:\Program Files\Solon\bin` en tête du PATH. Il contient le **CLI Docker officiel** et le
**plugin Compose** (Apache-2.0, versions dans `bin\NOTICE-third-party.txt`), derrière un petit lanceur `docker.exe`
qui les dirige vers le moteur Solon. Dans un **nouveau** terminal :

```powershell
docker version
docker compose -f examples\wordpress\compose.yaml up -d
```

Le lanceur respecte vos choix : `-H`, `--context`, `DOCKER_HOST` ou `DOCKER_CONTEXT` l'emportent, donc Docker
Desktop reste joignable si vous le gardez (`docker context use desktop-linux`).

Avec un CLI `docker` déjà installé (Docker Desktop, winget) :

```powershell
docker context create solon --docker host=npipe:////./pipe/solon
docker context use solon
```

> Si Docker Desktop est installé, le CLI utilise le gestionnaire d'identifiants Windows et peut envoyer
> des identifiants Docker Hub périmés (« unauthorized: incorrect username or password »). Faites
> `docker logout` ou utilisez un `DOCKER_CONFIG` vide pour le vérifier ; ce n'est pas lié à Solon.

## Dossiers Windows partagés : comment ça marche

Les dossiers Windows montés dans les conteneurs (`-v C:\...`, projets Compose) passent par **solonfs**, le système de
fichiers de Solon : un serveur côté Windows, un client côté Linux, et un protocole qui ramène tout un dossier en une
seule question au lieu d'une par fichier. Mesuré sur 5 000 fichiers face au partage 9P de Windows (celui de WSL2 et
de Docker Desktop) : lister 6,6× plus vite, lire les attributs 94× plus vite, lire le contenu 3,6 à 9× plus vite,
écrire 4× plus vite. Détails et méthode : `docs/measurements.md`.

Ce qu'il faut savoir :

- Les fichiers apparaissent comme appartenant à `root` en `0777` / `0666` ; `chmod` et `chown` sont acceptés et
  ignorés (Windows n'a pas de droits POSIX), comme avec Docker Desktop.
- Un changement fait côté Windows est vu dans le conteneur au plus 1,5 s plus tard.
- Pour les dépendances et les bases de données, préférez toujours des **volumes Docker** : ils vivent sur le disque de
  Solon, à vitesse native (20 à 50 ms pour les mêmes 5 000 fichiers).
- Repli : Réglages → « Utiliser l'ancien partage de fichiers Windows (9P) » remet l'ancien mécanisme au prochain
  démarrage du moteur. L'ancien partage reste aussi monté en secours sous `/mnt/host9p/<lettre>` dans la machine.

## Dépannage

Les messages de l'interface portent un **code stable** ; les journaux sont dans
`%ProgramData%\Solon\logs` (service) et se copient depuis l'écran d'erreur.

| Code | Cause | Que faire |
|---|---|---|
| `VIRTUALIZATION_DISABLED_IN_FIRMWARE` | VT-x / AMD-V désactivé | Activer la virtualisation dans le BIOS/UEFI (onglet Advanced, CPU ou Security), redémarrer. |
| `UNSUPPORTED_WINDOWS_EDITION` | Windows Famille | Passer à Windows Pro/Entreprise/Éducation. |
| `WINDOWS_FEATURE_MISSING` | Hyper-V ou Plateforme de machine virtuelle désactivés | Réinstaller Solon (l'installeur les active) ou, en PowerShell administrateur : `Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All`, puis redémarrer. |
| `WINDOWS_FEATURE_BLOCKED_BY_POLICY` | Stratégie d'entreprise (WSUS, GPO) refuse l'activation | Demander à l'administrateur d'activer `Microsoft-Hyper-V` et `VirtualMachinePlatform`. |
| `HYPERVISOR_NOT_RUNNING` | Hyperviseur Windows non démarré | Désinstaller les anciens VirtualBox/VMware (< 6.1 / < 15.5), vérifier `bcdedit /enum` (`hypervisorlaunchtype Auto`), ne pas exécuter Solon dans une VM sans virtualisation imbriquée. |
| `HOST_COMPUTE_SERVICE_UNAVAILABLE` | Service `vmcompute` ou `hns` arrêté ou absent | Redémarrer Windows ; sinon réinstaller Solon. |
| `BLOCKED_BY_SECURITY_SOFTWARE` | Antivirus / EDR bloque les disques ou le service | Ajouter `%ProgramData%\Solon` et le dossier d'installation aux exclusions. |
| `INSUFFICIENT_PRIVILEGES` | Le service ne tourne pas avec les droits attendus | Réinstaller Solon (le service doit tourner en LocalSystem). |
| `IMAGE_CORRUPTED` | Fichiers du moteur absents ou empreinte SHA-256 invalide | Réinstaller Solon. |
| `DATA_DISK_ERROR` | `data.vhdx` impossible à créer ou ouvrir | Vérifier l'espace disque et les exclusions antivirus ; en dernier recours renommer `%ProgramData%\Solon\data.vhdx` (perte des données Docker). |
| `VM_BOOT_TIMEOUT`, `AGENT_UNREACHABLE`, `ENGINE_UNREACHABLE` | La machine ne répond pas | Redémarrer le moteur ; consulter `solon-service.log` ; signaler avec le journal. |
| Pas d'accès réseau depuis les conteneurs | VPN d'entreprise ou conflit de plage IP | Solon choisit une plage libre parmi `172.30.0.0/24`… et fixe le MTU à 1400 ; certains VPN (AnyConnect, GlobalProtect) bloquent tout de même le trafic des cartes virtuelles : désactiver le VPN pour tester, puis signaler. |
| « Le service Solon n'est pas en cours d'exécution » | Service arrêté | `sc start SolonService` en administrateur, ou réinstaller. |

Coupure de courant ou arrêt brutal : au démarrage suivant, Solon vérifie et répare le disque de données
(`fsck`), puis redémarre le moteur. Les écritures non synchronisées des deux dernières secondes peuvent être
perdues, comme sur toute machine Linux.

## Limites connues (version 0.1)

- **Windows Famille** n'est pas pris en charge (composant Hyper-V absent).
- **Ports UDP** publiés non relayés vers `localhost` (TCP seulement).
- **Un seul moteur par machine**, pas de profils multiples.
- **Montages de dossiers Windows** : métadonnées lentes (voir plus haut). Les chemins `C:\...` de `docker run -v`,
  `--mount` et des fichiers Compose sont traduits automatiquement et le lecteur est partagé à la volée.
- **Docker Hub** : le CLI `docker` de Windows peut réutiliser des identifiants périmés stockés par Docker
  Desktop (voir « Utilisation »).
- **Installeur non signé** (avertissement SmartScreen) ; le chemin « activation de Hyper-V puis redémarrage »
  n'a pas encore été éprouvé sur une machine vierge.
- Pas de mise à jour automatique (aucune requête réseau) : installer la nouvelle version par-dessus.

## Documentation

- `ARCHITECTURE.md` : choix techniques (virtualisation HCS, image Linux, HvSocket, 9P, réseau), résultats
  mesurés bloc par bloc, registre des risques.
- `docs/measurements.md` : toutes les mesures et les faits établis.
- `bench/` : le script de comparaison et sa pile Compose (voir le comparatif plus haut).
- `site/` : le site de présentation (Astro, statique), publié sur GitHub Pages par le workflow « Site ».
- `tests/e2e/` : scénarios de bout en bout.
- `CONTRIBUTING.md` : comment contribuer.

## Licence

Apache 2.0, voir `LICENSE` et `NOTICE`. Solon embarque des logiciels tiers (CLI Docker, Docker Compose, un noyau
Linux, des paquets Alpine, des dépendances Rust et npm, la police Urbanist) sous leurs propres licences : voir
`THIRD-PARTY.md`. Pour contribuer : `CONTRIBUTING.md` ; pour signaler une faille : `SECURITY.md`.
