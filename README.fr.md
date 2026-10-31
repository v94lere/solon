# <img src="apps/desktop/src-tauri/icons/128x128.png" alt="Solon" width="60"> Solon

*English version: [README.md](README.md).*

[![CI](https://github.com/v94lere/solon/actions/workflows/ci.yml/badge.svg)](https://github.com/v94lere/solon/actions/workflows/ci.yml) [![Release](https://img.shields.io/github/v/release/v94lere/solon?label=release&color=f860c0)](https://github.com/v94lere/solon/releases/latest) [![Downloads](https://img.shields.io/github/downloads/v94lere/solon/total?color=e01818)](https://github.com/v94lere/solon/releases) [![License](https://img.shields.io/badge/license-Apache%202.0-blue)](LICENSE) [![Website](https://img.shields.io/badge/website-v94lere.github.io%2Fsolon-1a0d14)](https://v94lere.github.io/solon/)

**Docker sur Windows, en silence.** Solon est un moteur Docker autonome pour Windows : une installation,
aucun prérequis logiciel à installer soi-même, et un moteur Docker complet qui démarre en quelques secondes
dans une machine minuscule et invisible, gérée entièrement par Solon. Le vrai `docker` et le vrai
`docker compose`, pas de WSL, Windows Famille compris.

Solon **n'a pas de fenêtre**. Depuis la 0.2.0, c'est trois choses : un service Windows qui fait tourner le
moteur, une icône dans le coin de la barre des tâches (état, une entrée de menu par projet, notifications),
et une commande `solon` à côté de `docker` pour tout le reste, dont `solon why`, qui lit la Radiographie et
dit en français courant pourquoi un appel a échoué. L'application de bureau et ses écrans ont été retirés
en 0.2.0 ; ce qui remplace chaque écran est listé dans [Ce qui a changé en 0.2.0](#ce-qui-a-changé-en-020).

Solon **n'est pas** une interface pour un Docker Desktop déjà installé. Il remplace Docker Desktop :
moteur Docker, Compose, adresses https locales, dossiers partagés rapides, et quelques choses que rien
d'autre ne fait sur Windows.

> État du projet (31 octobre 2026) : **0.2.0**, dix-neuf versions en deux mois, écrit par une seule
> personne. Solon fait tourner mes propres projets tous les jours, et n'a été testé que sur deux machines :
> un PC Windows 11 Pro et une machine virtuelle Windows 11 Famille neuve. L'installeur n'est **pas encore
> signé** (avertissement SmartScreen ; empreinte SHA-256 publiée avec chaque version). Il reste sûrement des
> aspérités sur du matériel que je n'ai pas :
> [dites-moi ce qui casse chez vous](https://github.com/v94lere/solon/issues/1). Les idées et les
> contributions sont tout aussi bienvenues.

## Ce que Solon fait

- Démarre un moteur Docker (dockerd, containerd, runc, Compose) prêt en **~3 s** après l'ordre de démarrage.
- Expose l'API Docker sur `\\.\pipe\solon` ; les commandes `docker` et `docker compose` livrées avec Solon
  sont déjà dans votre `PATH` et pointent vers Solon, et tout autre CLI Docker fonctionne avec un
  `docker context`.
- Relaie les ports publiés vers `localhost` sans configuration.
- Partage vos dossiers Windows à la demande : `docker run -v C:\...`, `--mount`, projets Compose, exactement
  comme avec Docker Desktop, par **solonfs**, le partage de fichiers de Solon, mesuré 3,6 à 94× plus rapide
  que le partage 9P de WSL2 et de Docker Desktop selon l'opération (`docs/measurements.md`).
- **Import depuis Docker Desktop** : `solon import` liste ses volumes et ses images avec leur taille et
  copie ceux que vous nommez dans le moteur de Solon. Rien n'est modifié ni supprimé du côté de Docker
  Desktop, et ce qui porte déjà le même nom dans Solon n'est pas touché.
- **Des adresses qui marchent toujours** : chaque conteneur en marche est joignable sur
  `https://<nom>.solon.local` (et `https://<service>.<projet>.solon.local` pour Compose), **qu'il publie un
  port ou non**, avec un certificat reconnu par le navigateur (une autorité de certification locale créée à
  la première utilisation). Les adresses des conteneurs (`10.90.x.y`) sont aussi joignables directement
  depuis Windows.
- **Réveil à la demande** : un conteneur joint via Solon qui ne reçoit plus rien pendant dix minutes est mis
  en pause (zéro processeur, mémoire conservée) ; la requête suivante le réveille en une fraction de seconde
  avant d'être servie. Les bases de données et les tâches de fond qui ne parlent qu'en interne ne sont jamais
  concernées.
- **Radiographie** : un journal de chaque échange dans vos projets — connexions TCP avec leur issue,
  échanges HTTP complets pour ce qui entre par une adresse `*.solon.local`, noms sur les adresses
  extérieures. `solon trace` l'imprime, `solon why` le lit et dit en français courant pourquoi un appel a
  échoué. Les valeurs sensibles sont masquées avant d'être gardées ; les corps sont désactivés par défaut et
  s'activent par projet.
- **Une commande `solon`** à côté de `docker` : état du moteur et adresses, démarrage et arrêt avec les étapes
  affichées, réglages, sauvegarde et restauration d'un projet, recherche des projets sur ce PC, import depuis
  Docker Desktop, un environnement par branche Git, un shell root dans le moteur, mise à jour. Elle parle au
  service par son pipe local : aucune demande d'administrateur, et aucune requête sortante hors
  `solon update`, que vous lancez vous-même.
- **Une icône dans le coin de la barre des tâches** : état du moteur, nombre de conteneurs en marche, un
  sous-menu par projet Compose (ouvrir son adresse, démarrer, redémarrer, arrêter), un terminal avec `solon`
  et `docker` prêts, des notifications Windows quand quelque chose se passe.
- **Relance de ce qui tournait** : quand le moteur s'arrête (redémarrage de Windows, mise à jour,
  `solon stop`), Solon retient les projets et conteneurs en marche et les redémarre une fois le moteur revenu.
- Mémoire au repos (moteur + service) : **430 à 520 Mo** mesurés pour 2 Go alloués ; la mémoire inutilisée
  est rendue à Windows automatiquement.
- **Aucune télémétrie, aucune requête réseau sortante** en dehors de ce que vos conteneurs et vos
  `docker pull` demandent, et de `solon update`, qui demande à github.com la dernière version seulement quand
  vous le lancez.

## Comparatif : performances, empreinte et fonctionnalités

### Performances : Docker Desktop et Solon

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

### Empreinte

Solon mesuré le 13 septembre 2026 (0.1.10, trois conteneurs en marche) ; Docker Desktop mesuré sur la même machine
le 3 septembre ; les deux autres d'après leur documentation (marqués *publié*).

| | Solon | Docker Desktop | Podman Desktop | Rancher Desktop |
|---|---|---|---|---|
| Installateur | **102 Mo** | ≈ 600 Mo *(publié)* | ≈ 100 Mo + machine WSL *(publié)* | ≈ 500 Mo *(publié)* |
| Installé sur le disque | **406 Mo** (image Linux 310, CLI `docker` 73, Solon 22) | 6 Go requis *(publié)* | 500 Mo + 2 à 10 Go d'images *(publié)* | non publié |
| Mémoire au repos, aucun conteneur | **430 à 520 Mo** | 1 998 Mo | machine WSL 2, jusqu'à 50 % de la RAM par défaut *(publié)* | moitié des cœurs et de la RAM par défaut *(publié)* |
| Mémoire avec une application web + PostgreSQL | **1 196 Mo** | 5 146 Mo | non mesuré | non mesuré |
| Exige WSL 2 | non (machine Hyper-V propre) | oui (ou ancien moteur Hyper-V) | oui | oui |

### Fonctionnalités comparées

Solon en 0.2.0 ; les autres d'après leur documentation en septembre 2026. « Partiel » : la fonction existe avec une réserve.

| | Solon | Docker Desktop | Podman Desktop | Rancher Desktop |
|---|---|---|---|---|
| CLI `docker` et Compose officiels, sans alias | oui | oui | partiel : socket compatible, quelques différences | oui (moby) |
| Fonctionne sans WSL 2 | oui | partiel : ancien moteur Hyper-V | non | non |
| Windows Famille | oui (Plateforme de machine virtuelle) | oui (WSL 2) | oui | oui |
| `https://<service>.<projet>.solon.local` pour chaque conteneur, certificat local reconnu | oui | non | non | non |
| Interface graphique (fenêtre, écrans) | non, par choix : une icône dans la barre des tâches et la commande `solon` | oui | oui | oui |
| Projets Compose listés avec leurs adresses, démarrés et arrêtés depuis l'icône de la barre des tâches | oui | partiel : liste Compose | partiel : liste Compose | non |
| Un environnement par branche Git, volumes copiés | oui (`solon clone`) | non | non | non |
| Sauvegarde et restauration d'un projet, volumes compris | oui (`solon backup`, `solon restore`) | partiel : extension tierce | non | non |
| Ligne de commande pour le moteur lui-même : état, démarrage et arrêt, réglages, sauvegarde, pourquoi un appel a échoué, mise à jour | oui (`solon`) | partiel : `docker desktop start/stop/status` *(documenté)* | partiel : `podman machine` *(documenté)* | partiel : `rdctl` *(documenté)* |
| Dit pourquoi un appel entre conteneurs a échoué, en français courant | oui (`solon why`) | non | non | non |
| Conteneurs inactifs mis en veille, réveil à la demande | oui | partiel : Resource Saver met toute la machine en pause | non | non |
| Dossiers Windows partagés par solonfs (3,6 à 94× plus rapide que 9P selon l'opération) | oui | non : 9P via WSL 2 | non : 9P | non : 9P |
| Recherche des projets Compose sur le disque (7 s sur 928 Go) | oui (`solon scan`) | non | non | non |
| Projets relancés après un redémarrage de Windows ou une mise à jour | oui | partiel : politique `restart` de Docker | partiel | partiel |
| VS Code Dev Containers | oui, vérifié | oui | partiel : réglage du chemin docker | oui |
| Kubernetes intégré | non | oui | partiel : extensions Kind / Minikube | oui (K3s) |
| Conteneurs Windows | non | oui | non | non |
| Place de marché d'extensions | non | oui | oui | non |
| Gratuit en entreprise | oui (Apache 2.0) | payant au-delà de 250 salariés ou 10 M$ de CA | oui (Apache 2.0) | oui (Apache 2.0) |
| Installateur signé | pas encore | oui | oui | oui |

## Prérequis

| Prérequis | Détail |
|---|---|
| Windows | **Windows 11 ou Windows 10 22H2, toutes éditions, Famille comprise.** Pro, Entreprise et Éducation utilisent Hyper-V ; Famille utilise la Plateforme de machine virtuelle, que l'installateur active (un redémarrage). Vérifié sur Windows 11 Famille 25H2. Sur Famille, le repli de partage de fichiers 9P n'est pas disponible ; le partage par défaut fonctionne. |
| Processeur | Virtualisation matérielle **activée dans le BIOS/UEFI** : Intel VT-x (souvent « Intel Virtualization Technology ») ou AMD-V (« SVM Mode »). |
| Mémoire | 8 Go recommandés (4 Go minimum). Solon alloue 2 Go au moteur par défaut, ajustable par `solon settings memory=…`. |
| Disque | ~400 Mo pour Solon et son image Linux, plus un disque de données dynamique (64 Go maximum par défaut, occupé à la demande). |
| Composants Windows | « Hyper-V » et « Plateforme de machine virtuelle ». L'installeur les active ; un redémarrage peut être demandé. |

Solon cohabite avec WSL2 et Docker Desktop : il n'utilise ni leur pipe, ni leurs réseaux, ni leur
contexte `docker` par défaut.

## Installation

### Installeur

Téléchargez `Solon_<version>_x64-setup.exe` depuis les [releases](https://github.com/v94lere/solon/releases)
et vérifiez son empreinte SHA-256 avec la valeur publiée avec la version. L'installeur demande l'élévation une
fois, puis : active les composants Windows nécessaires (un redémarrage peut être demandé), installe le service
`SolonService`, copie l'image Linux et les commandes `docker` et `solon`, et crée le raccourci. Installation silencieuse :
`Solon_<version>_x64-setup.exe /S`. Installer la 0.2.0 par-dessus une 0.1.x conserve tout : images, conteneurs,
volumes, réglages, historique de la Radiographie.

L'installeur **n'est pas encore signé** : SmartScreen affiche « Windows a protégé votre ordinateur » ; cliquez
« Informations complémentaires » puis « Exécuter quand même ». La chaîne de signature est prête
(`installer/sign.ps1`, `tauri.signed.conf.json`) et s'activera dès qu'un certificat sera disponible.

Désinstallation : Paramètres → Applications → Solon. Le désinstalleur arrête le moteur, retire le service et
**demande** avant de supprimer vos images et volumes (`%ProgramData%\Solon`) ; par défaut ils sont conservés.

### Depuis les sources (développeurs)

Prérequis : Rust stable (≥ 1.85), Node 22 (seulement pour l'empaqueteur Tauri : il ne reste aucun JavaScript
dans Solon), et **Solon lui-même** installé (l'image Linux du moteur se construit dans un conteneur Solon : pas
besoin de WSL). Une session Windows **administrateur** sert seulement au mode console.

```powershell
# 1. Agent invité (compilation croisée depuis Windows, sans chaîne C)
rustup target add x86_64-unknown-linux-musl
cargo build -p solon-agent --release --target x86_64-unknown-linux-musl

# 2. Image Linux (noyau réutilisé + système racine Alpine + initrd), construite dans un conteneur Solon (~15 s)
docker run --rm -v "${PWD}:/work" -w /work public.ecr.aws/docker/library/alpine:3.24 sh -c "apk add -q bash curl python3 e2fsprogs coreutils tar grep findutils gzip; SKIP_KERNEL=1 SOLON_IMAGE_VERSION=0.1.0-dev.N bash image/build.sh /work/target/x86_64-unknown-linux-musl/release/solon-agent"
#    (le noyau lui-même se compile une fois avec image/kernel/build-kernel.sh, ~7 min, dans le même genre de conteneur)

# 3. Service, commande `solon` et icône de la barre des tâches (l'installeur embarque image/out/<version>, voir tauri.conf.json)
cargo build --release -p solon-service -p solon-cli
cd apps\desktop && npm install && npm run tauri build
#    (le service et le CLI sont copiés comme ressources : `tauri build` ne les reconstruit pas, d'où l'ordre)
```

Mode développement sans installer : le service se lance en mode console, à la main
(`solon-service.exe console --start` dans un terminal administrateur) ou par le script ci-dessous (une
fenêtre UAC), puis la commande depuis `target\release` et l'icône en mode dev :

```powershell
.\tests\e2e\start-console.ps1 -ImageDir .\image\out\<version> -Release
.\target\release\solon-cli.exe status
cd apps\desktop; npm run tauri dev
```

## Utilisation

Il n'y a rien à ouvrir. Après l'installation, le moteur démarre, `docker` fonctionne dans tout nouveau
terminal, et l'icône de Solon est dans le coin de la barre des tâches. Tout le reste est une des commandes
`solon` ci-dessous.

### L'icône

Un clic, gauche ou droit, ouvre un menu natif :

- **Moteur : en marche / arrêté / en échec…** Cliquer dessus ouvre un terminal.
- **N conteneurs en marche.**
- **Un sous-menu par projet Compose**, « ● blog (2/2) » : *Ouvrir https://web.blog.solon.local*, *Démarrer
  le projet*, *Redémarrer le projet*, *Arrêter le projet*. Les conteneurs lancés hors projet ont le même
  sous-menu (Ouvrir, Démarrer, Redémarrer, Arrêter).
- **Ouvrir un terminal (solon, docker)** : une console dans votre dossier personnel, avec `solon` et `docker`
  dans le PATH.
- **Pourquoi ça casse ? (solon why)** : le même terminal, la réponse déjà affichée.
- **Mettre à jour Solon…** : lance `solon update`.
- **Démarrer avec Windows** : au premier lancement, Solon s'inscrit au démarrage de votre session Windows
  (clé Run de votre compte) et une notification le signale, parce qu'une icône absente quand le moteur tombe
  ne sert à rien. Décocher la case, c'est dire non ; ce non est retenu, Solon ne se réinscrit pas. La
  désinstallation retire l'entrée.
- **Démarrer le moteur** / **Arrêter le moteur**, **Quitter Solon**.

Le point de l'icône est vert, orange, rouge ou gris selon l'état du moteur. Notifications Windows, rares
par choix : moteur en échec, redémarrage de Docker, disque du moteur presque plein, projets relancés après
le démarrage du moteur, action du menu qui a échoué. Lancer `Solon.exe` une seconde fois ne fait que
notifier « Solon tourne déjà ». Les libellés sont en anglais, ou en français quand Windows l'est
(`SOLON_LANG=fr` force).

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

### Ligne de commande : `solon`

Le même dossier contient `solon`, la commande propre à Solon. `docker` parle aux conteneurs ; `solon` parle au
moteur qui les entoure : son état, ses adresses, ses réglages, la Radiographie, les sauvegardes, les mises à
jour. Elle passe par le pipe local du service : aucune demande d'administrateur, aucune requête sortante (hors
`solon update`, voir plus bas). Les messages sont en anglais, ou en français quand Windows l'est
(`SOLON_LANG=fr` force). `solon help` liste toutes les commandes ; `--json` sur `status`, `projects`,
`domains`, `trace`, `settings`, `prereq`, `watch`, `scan`, `import` et `update` donne une sortie lisible par
un programme. Codes de sortie : 0, 1 erreur, 2 usage, 3 service injoignable.

| Commande | Ce qu'elle fait |
|---|---|
| `solon` ou `solon status` | État du moteur, image, adresse, domaines locaux, puis les conteneurs en marche avec leur projet, leur état (« endormi » si Solon l'a mis en veille) et leur adresse `https://service.projet.solon.local`. |
| `solon start`, `solon stop [--force]`, `solon restart` | Démarrer ou arrêter le moteur, chaque étape affichée. |
| `solon projects` | Les projets Compose connus du moteur, avec leur dossier Windows et leurs adresses. |
| `solon domains` | Toutes les adresses `*.solon.local` qui répondent en ce moment. |
| `solon why [nom]` | Ce qui ne va pas, en français courant, à partir de la Radiographie. |
| `solon trace [nom] [-n N \| --all] [-f]` | Le journal des échanges, 50 dernières lignes par défaut ; `-f` suit le direct. |
| `solon clear [projet]` | Oublier l'historique observé, d'un projet ou de tout. |
| `solon backup [dossier] [fichier.zip] [--force]` | Un zip avec les fichiers Compose, le `.env` et chaque volume du projet, en marche ou non. |
| `solon restore <fichier.zip> [dossier]` | Recrée les volumes et les fichiers ; il reste `docker compose up -d`. |
| `solon clone <source> <cible>` | Copie les volumes d'un projet sous un autre nom : un environnement par branche Git (`blog` → `blog-feature-login`), rien n'est écrasé. |
| `solon scan` | Trouver les projets Docker de ce PC : le service lit la table des fichiers des disques internes (quelques secondes, rien ne quitte le PC) et liste chaque dossier avec un `compose.yaml`, un `Dockerfile` ou un `devcontainer.json`. |
| `solon import [--all \| --volumes a,b --images x:tag]` | Copier volumes et images depuis un Docker Desktop en marche, sans y toucher ; sans option, lister ce qu'il contient. |
| `solon shell` | Un shell root dans le moteur (`docker`, `ps`, `df`, `dmesg`…), par un conteneur busybox privilégié et `nsenter`. |
| `solon exec <commande…>` | Une commande en root dans le moteur ; un seul mot entre guillemets vaut une ligne shell. |
| `solon settings [clé=valeur…]` | Voir ou changer les réglages : `memory` (Mo), `cpus`, `disk` (Gio), `autostart`, `resume`, `sleep`, `idle` (min), `never` (liste ou `none`), `bodies` (liste ou `none`), `legacy-9p`. Mémoire, processeurs, disque et partage s'appliquent au prochain démarrage du moteur. |
| `solon update [--check]` | La dernière version publiée sur GitHub ; sans `--check`, télécharge l'installateur, vérifie son SHA-256 contre le `SHA256SUMS.txt` de la version et le lance (Windows demande l'autorisation). C'est la seule requête sortante de Solon, et seulement quand vous la demandez. Depuis le Microsoft Store, répond « les mises à jour viennent du Store ». |
| `solon prereq` | Les prérequis Windows, ligne par ligne. |
| `solon logs [-n N] [-f]`, `solon diag [fichier.zip]` | Le journal du service ; le zip à joindre à un signalement (journaux, état, réglages, prérequis, `docker info` ; aucun identifiant). |
| `solon watch [--json]` | Les événements du moteur au fil de l'eau. |
| `solon version` | Les versions de la commande, du service et de l'image. |

`solon` seul vaut `solon status`. Les sorties ci-dessous sont abrégées :

```text
> solon
Solon 0.2.0 · moteur prêt depuis 09:12:04 (2 h 31 min)
  image 0.1.0-dev.22 · adresse 172.30.0.2
  domaines locaux *.solon.local : http et https

2 conteneurs en marche, 1 endormis (réveil à la demande)
CONTENEUR              PROJET     ÉTAT        ADRESSE
wordpress-db-1         wordpress  Up 2 hours  https://db.wordpress.solon.local
wordpress-wordpress-1  wordpress  endormi     https://wordpress.wordpress.solon.local

> solon restart
Redémarrage du moteur…
  vérification des prérequis…
  nettoyage…
  vérification de l'image…
  préparation du disque de données…
  création du réseau…
  création du moteur…
  amorçage…
  attente du moteur…
  configuration du réseau…
  attente de Docker…
Moteur prêt.

> solon projects
PROJET     ÉTAT        DOSSIER                    ADRESSES
wordpress  running(2)  C:\Users\moi\wordpress     db.wordpress.solon.local, wordpress.wordpress.solon.local

> solon why wordpress
Depuis 01:27:40, … Ce qui ressort :

- 10.90.5.4 a essayé 4 fois de joindre db (wordpress) sur le port 9999 sans jamais obtenir de réponse (la
  dernière à 01:28:51, depuis le programme « wget »). Rien n'écoutait là. Le conteneur est-il démarré, et
  écoute-t-il bien ce port ? `docker logs` dessus dit en général pourquoi.

Le détail : solon trace wordpress

> solon trace wordpress -n 3
01:28:25  wordpress (wordpress) -> db (wordpress):3306  fermée après 48 ms  [apache2]
01:28:25  GET https://wordpress.wordpress.solon.local/wp-login.php  -> 302 en 170 ms  (wordpress (wordpress))
01:28:51  10.90.5.4 -> db (wordpress):9999  ÉCHEC, pas de réponse  [wget]

> solon backup C:\Users\moi\wordpress
Sauvegarde de wordpress (C:\Users\moi\wordpress)…
Écrit : wordpress-20261031-1422.zip (2 volumes, …)

> solon settings memory=4096 idle=20
Enregistré.
Mémoire, processeurs, disque et partage de fichiers s'appliquent au prochain démarrage du moteur : solon restart
```

`solon why` lit l'historique de la Radiographie et regroupe ce qui a mal tourné : les appels
refusés par appelant, destination et port, avec un conseil (conteneur endormi, rien n'écoute, adresse hors du
moteur), les erreurs de serveur (5xx) par hôte, les refus 401/403, la requête la plus lente quand elle dépasse
une seconde. Quand rien ne ressort, il le dit, avec les comptes. `solon trace` est le journal (`-f` suit le
direct, `--all` imprime tout), `solon clear [projet]` l'oublie.

`solon backup` écrit un zip avec le fichier Compose, le `.env` et les volumes ; `solon restore` le remet dans
le dossier de votre choix, sur le même PC ou un autre. `solon settings` seul montre chaque réglage avec sa
signification.

### Ce qui a changé en 0.2.0

L'application de bureau (fenêtre, écrans) a été retirée le 31 octobre 2026. Le moteur, le service, vos
données (images, conteneurs, volumes, réglages, historique) et la commande `docker` n'ont pas bougé. Ce que
faisait chaque écran, et ce qui le fait maintenant :

| Disparu | À la place |
|---|---|
| Écrans Conteneurs, Images, Volumes, Réseaux | `docker ps`, `docker images`, `docker volume`, `docker network` |
| Page Projets : cartes, éditeur de compose et de `.env` avec vérification des ports, journaux filtrés, panneau Fichiers, modèles de projets, recherche sur le PC, import Docker Desktop, un environnement par branche, sauvegarde et restauration | `solon projects`, `solon scan`, `solon import`, `solon clone`, `solon backup` / `solon restore`, votre éditeur, `docker compose logs`, `docker cp` |
| Terminal dans la machine | `solon shell` |
| Écran Activité (courbes processeur, mémoire, disque, réseau) | `docker stats` ; pas d'équivalent graphique |
| Écran Radiographie (carte, journal, barre de lecture, corps par projet) | `solon trace`, `solon why`, `solon settings bodies=…` ; la carte et la relecture visuelle n'ont plus d'équivalent pour l'instant : une extension VS Code est l'étape suivante |
| Écran Réglages | `solon settings` |
| Palette de commandes, `Ctrl+Alt+S`, visite de bienvenue, thème et couleur d'accent, bannière de mise à jour | disparus |

### VS Code Dev Containers

L'extension **Dev Containers** fonctionne avec Solon telle quelle : elle utilise le `docker` de votre PATH,
qui est celui de Solon. Ouvrez un dossier qui contient `.devcontainer/devcontainer.json`, lancez « Dev Containers :
Rouvrir dans un conteneur », et VS Code construit ou tire l'image, démarre le conteneur avec le dossier monté
dans `/workspaces/<nom>`, installe son serveur dedans et s'y connecte. Les ports transmis fonctionnent, le
`postCreateCommand` s'exécute, et le conteneur apparaît dans `solon status` (et son dossier dans
`solon scan`). Rien à configurer ; si vous aviez réglé `dev.containers.dockerPath` pour
Docker Desktop, retirez-le. Testé avec `@devcontainers/cli` 0.89 et l'image
`mcr.microsoft.com/devcontainers/base:alpine`.

La vue **Conteneurs** (extension `ms-azuretools.vscode-containers`) se rafraîchit en direct grâce à
`docker events`. L'extension arme cette écoute une fois, au démarrage, et renonce après trois échecs
immédiats. Le pipe Docker de Solon existe dès le lancement du service Solon et `docker` attend pendant que le
moteur démarre : une fenêtre VS Code restaurée à l'ouverture de session garde le rafraîchissement en direct.
Si Solon est mis à jour pendant que VS Code est ouvert, le pipe disparaît quelques secondes et cette fenêtre
retombe sur un rafraîchissement par minute et le bouton Actualiser : lancez *Developer: Reload Window* pour
retrouver le direct.

## Dossiers Windows partagés : comment ça marche

Les dossiers Windows montés dans les conteneurs (`-v C:\...`, projets Compose) passent par **solonfs**, le système de
fichiers de Solon : un serveur côté Windows, un client côté Linux, et un protocole qui ramène tout un dossier en une
seule question au lieu d'une par fichier. Mesuré sur 5 000 fichiers face au partage 9P de Windows (celui de WSL2 et
de Docker Desktop) : 3,6 à 94× plus rapide selon l'opération (lister 6,6×, lire les attributs 94×, lire le contenu
3,6 à 9×, écrire 4×). Détails et méthode : `docs/measurements.md`.

Ce qu'il faut savoir :

- Les fichiers apparaissent comme appartenant à `root` en `0777` / `0666` ; `chmod` et `chown` sont acceptés et
  ignorés (Windows n'a pas de droits POSIX), comme avec Docker Desktop.
- Un changement fait côté Windows est vu dans le conteneur au plus 1,5 s plus tard.
- Pour les dépendances et les bases de données, préférez toujours des **volumes Docker** : ils vivent sur le disque de
  Solon, à vitesse native (20 à 50 ms pour les mêmes 5 000 fichiers).
- Repli : `solon settings legacy-9p=on` remet l'ancien mécanisme au prochain démarrage du moteur. L'ancien partage reste aussi monté en secours sous `/mnt/host9p/<lettre>` dans la machine.

## Dépannage

Guide pas à pas des problèmes les plus fréquents (SmartScreen, redémarrage après l'activation d'Hyper-V, port
déjà pris, conteneur qui se termine aussitôt, réseau et adresses `solon.local`, espace disque) :
**<https://v94lere.github.io/solon/fr/depannage/>**. Pour signaler un problème, joignez le zip écrit par
`solon diag` (journaux, état, réglages, `docker info` ; aucun identifiant).

Les messages d'erreur portent un **code stable**, affiché par `solon status` et dans la notification de la
barre des tâches ; les journaux sont dans `%ProgramData%\Solon\logs` (service, `solon logs`).

| Code | Cause | Que faire |
|---|---|---|
| `VIRTUALIZATION_DISABLED_IN_FIRMWARE` | VT-x / AMD-V désactivé | Activer la virtualisation dans le BIOS/UEFI (onglet Advanced, CPU ou Security), redémarrer. |
| `WINDOWS_FEATURE_MISSING` | Hyper-V ou Plateforme de machine virtuelle désactivés | Réinstaller Solon (l'installeur les active) ou, en PowerShell administrateur : `Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V,VirtualMachinePlatform -All`, puis redémarrer. |
| `WINDOWS_FEATURE_BLOCKED_BY_POLICY` | Stratégie d'entreprise (WSUS, GPO) refuse l'activation | Demander à l'administrateur d'activer `Microsoft-Hyper-V` et `VirtualMachinePlatform`. |
| `HYPERVISOR_NOT_RUNNING` | Hyperviseur Windows non démarré | Désinstaller les anciens VirtualBox/VMware (< 6.1 / < 15.5), vérifier `bcdedit /enum` (`hypervisorlaunchtype Auto`), ne pas exécuter Solon dans une VM sans virtualisation imbriquée. |
| `HOST_COMPUTE_SERVICE_UNAVAILABLE` | Service `vmcompute` ou `hns` arrêté ou absent | Redémarrer Windows ; sinon réinstaller Solon. |
| `BLOCKED_BY_SECURITY_SOFTWARE` | Antivirus / EDR bloque les disques ou le service | Ajouter `%ProgramData%\Solon` et le dossier d'installation aux exclusions. |
| `INSUFFICIENT_PRIVILEGES` | Le service ne tourne pas avec les droits attendus | Réinstaller Solon (le service doit tourner en LocalSystem). |
| `IMAGE_CORRUPTED` | Fichiers du moteur absents ou empreinte SHA-256 invalide | Réinstaller Solon. |
| `DATA_DISK_ERROR` | `data.vhdx` impossible à créer ou ouvrir | Vérifier l'espace disque et les exclusions antivirus ; en dernier recours renommer `%ProgramData%\Solon\data.vhdx` (perte des données Docker). |
| `VM_BOOT_TIMEOUT`, `AGENT_UNREACHABLE`, `ENGINE_UNREACHABLE` | La machine ne répond pas | Redémarrer le moteur ; consulter `solon-service.log` ; signaler avec le journal. |
| `UNSUPPORTED_WINDOWS_EDITION` | Édition de Windows non prise en charge | Solon fonctionne sur Windows 10 22H2 et Windows 11, toutes éditions, Famille comprise ; les versions plus anciennes ne sont pas prises en charge. Mettre Windows à jour. |
| `VM_CONFIGURATION_REJECTED`, `HCS_ERROR` | Windows a refusé la configuration du moteur, ou la couche de virtualisation a renvoyé une erreur | Redémarrer une fois ; si cela persiste, signaler avec `solon-service.log` (il contient le code d'erreur Windows). |
| `IO` | Un fichier sous `%ProgramData%\Solon` n'a pas pu être lu ou écrit | Vérifier l'espace disque et les exclusions antivirus sur `%ProgramData%\Solon`. |
| `INTERNAL` | Un bogue de Solon | Signaler avec `solon-service.log` ; le moteur repart en général après un redémarrage. |
| Pas d'accès réseau depuis les conteneurs | VPN d'entreprise ou conflit de plage IP | Solon choisit une plage libre parmi `172.30.0.0/24`… et fixe le MTU à 1400 ; certains VPN (AnyConnect, GlobalProtect) bloquent tout de même le trafic des cartes virtuelles : désactiver le VPN pour tester, puis signaler. |
| « Le service Solon n'est pas en cours d'exécution » | Service arrêté | `sc start SolonService` en administrateur, ou réinstaller. |

Coupure de courant ou arrêt brutal : au démarrage suivant, Solon vérifie et répare le disque de données
(`fsck`), puis redémarre le moteur. Les écritures non synchronisées des deux dernières secondes peuvent être
perdues, comme sur toute machine Linux.

## Limites connues (version 0.2)

- **Pas d'interface graphique** : ni fenêtre, ni courbes, ni carte. Un choix assumé pour la 0.2.0 : Solon,
  c'est le moteur, l'icône et la commande ; la partie visuelle de la Radiographie reviendra sous la forme
  d'une extension VS Code.
- **Ports UDP** publiés non relayés vers `localhost` (TCP seulement).
- **Un seul moteur par machine**, pas de profils multiples.
- **Installeur non signé** (avertissement SmartScreen). **Pas de mise à jour automatique, pas même de
  vérification** : `solon update --check` demande à github.com si une version plus récente existe,
  `solon update` (ou « Mettre à jour Solon… » dans le menu de l'icône) télécharge l'installateur, vérifie
  son SHA-256 avec le `SHA256SUMS.txt` de la version, puis Windows demande l'autorisation ; le service
  relance ce qui tournait. Rien ne s'installe sans vous.
- Un conteneur endormi n'exécute pas ses tâches planifiées internes tant que rien ne l'appelle ;
  `solon settings never=<nom>` l'exclut de la mise en veille si cela compte.
- `curl.exe` de Windows refuse les certificats HTTPS locaux sans `--ssl-no-revoke` (comme avec mkcert) ; les
  navigateurs et .NET les acceptent.

## Où ça va, et où vous pouvez aider

Solon est jeune, et la liste de ce qui reste à faire est plus longue que celle de ce qui est fait. Voici les
prochains chantiers, chacun avec une issue où commenter ou se lancer :

- **Une extension VS Code** pour la Radiographie : la carte de qui appelle qui et la barre de lecture,
  parties avec la fenêtre en 0.2.0, ont leur place à côté du code.
- **Mettre à jour les images en une commande** ([#3](https://github.com/v94lere/solon/issues/3)) : dire
  qu'une version plus récente existe, la tirer et recréer les conteneurs.
- **Relayer les ports UDP publiés** ([#5](https://github.com/v94lere/solon/issues/5)), le dernier écart face à
  `docker run -p`.
- **Rechargement à chaud depuis Windows** ([#7](https://github.com/v94lere/solon/issues/7)) : prévenir le
  conteneur qu'un fichier Windows a changé, pour que Vite, nodemon et le rechargeur d'Odoo le voient.
- **Windows 10**, testé et non supposé.
- **Un installateur signé** : le sponsor de certificat demande d'abord de la notoriété, donc celui-ci dépend
  des gens qui trouveront le projet utile.

Pour commencer, les issues marquées [good first issue](https://github.com/v94lere/solon/labels/good%20first%20issue)
et [help wanted](https://github.com/v94lere/solon/labels/help%20wanted). `CONTRIBUTING.md` compile le tout en
une commande, et la documentation, les traductions et les reproductions de bogues comptent autant que le code.

## Documentation

- `ARCHITECTURE.md` : choix techniques (virtualisation HCS, image Linux, HvSocket, solonfs, réseau), résultats
  mesurés bloc par bloc, registre des risques.
- `docs/guide-fonctionnel.md` : le guide fonctionnel : l'icône, les commandes, ce que le moteur fait tout seul.
- `docs/radiographie.md` : la Radiographie, phase par phase.
- `docs/measurements.md` : toutes les mesures et les faits établis.
- `bench/` : le script de comparaison et sa pile Compose (voir le comparatif plus haut).
- `site/` : le site de présentation (Astro, statique), publié sur GitHub Pages par le workflow « Site ».
- `tests/e2e/` : scénarios de bout en bout.
- `CONTRIBUTING.md` : comment contribuer ; `SECURITY.md` : comment signaler une faille ;
  `CODE_OF_CONDUCT.md`.

## Licence

Apache 2.0, voir `LICENSE` et `NOTICE`. Solon embarque des logiciels tiers (CLI Docker, Docker Compose, un noyau
Linux, des paquets Alpine, des dépendances Rust) sous leurs propres licences : voir `THIRD-PARTY.md`. Pour contribuer : `CONTRIBUTING.md` ; pour signaler une faille : `SECURITY.md`.
