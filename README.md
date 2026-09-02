# Solon

**Solon est un gestionnaire de conteneurs autonome pour Windows**, dans l'esprit d'OrbStack : une
installation, aucun prérequis logiciel à installer soi-même, et un moteur Docker complet qui démarre
en quelques secondes dans une machine minuscule et invisible, gérée entièrement par Solon.

Solon **n'est pas** une interface pour un Docker Desktop déjà installé. Il remplace Docker Desktop :
moteur Docker, Compose, images, volumes, réseaux, terminal, journaux, et une icône dans la barre des tâches.

> État du projet (3 septembre 2026) : **MVP 0.1.0**. Les sept fonctionnalités sont opérationnelles et
> l'installeur a été testé sur la machine de développement (Windows 11 Pro). Non signé pour l'instant.

## Ce que Solon fait

- Démarre un moteur Docker (dockerd, containerd, runc, Compose) prêt en **~2,5 s** après l'ordre de démarrage.
- Expose l'API Docker sur `\\.\pipe\solon` : l'interface Solon et le CLI `docker` que vous avez déjà
  fonctionnent (`docker -H npipe:////./pipe/solon ps`, ou un contexte `docker context create`).
- Relaie les ports publiés vers `localhost` sans configuration.
- Partage vos dossiers Windows à la demande (projets Compose, montages `-v C:\...`).
- Mémoire au repos (moteur + service) : **430 à 520 Mo** mesurés pour 2 Go alloués.
- **Aucune télémétrie, aucune requête réseau sortante** en dehors de ce que vos conteneurs et vos
  `docker pull` demandent.

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

Prérequis : Rust stable (≥ 1.85), Node 22, WSL 2 avec une distribution Ubuntu (pour construire l'image
Linux), une session Windows **administrateur** pour lancer le service.

```powershell
# 1. Image Linux (noyau + système racine + initrd), depuis WSL en root
wsl -u root -e bash -c "cd /mnt/c/chemin/vers/solon && bash image/build.sh /mnt/c/chemin/vers/solon/target/x86_64-unknown-linux-musl/release/solon-agent"

# 2. Agent invité (compilation croisée depuis Windows, sans chaîne C)
rustup target add x86_64-unknown-linux-musl
cargo build -p solon-agent --release --target x86_64-unknown-linux-musl

# 3. Service et application
cargo build --release -p solon-service
cd apps\desktop && npm install && npm run tauri build
```

Pour le développement, le service se lance en mode console (fenêtre UAC) et l'application en mode dev :

```powershell
.\tests\e2e\start-console.ps1 -ImageDir .\image\out\<version> -Release
cd apps\desktop; npm run tauri dev
```

## Utilisation

- **Barre d'état** : état du moteur, démarrer / redémarrer / arrêter.
- **Conteneurs** : liste temps réel avec CPU et mémoire, filtre, groupes Compose, actions, journaux en
  flux, terminal, inspection.
- **Compose** : « Open a project… », choisissez le dossier contenant `compose.yaml`, puis Up / Down / Status.
- **Images, Volumes, Réseaux** : liste, création, inspection, suppression (toujours avec confirmation).
- **Réglages** : langue (anglais par défaut, français), mémoire et processeurs du moteur, limite de
  stockage, démarrage à l'ouverture de session.
- **Barre des tâches** : un clic sur l'icône ouvre le menu : état du moteur, nombre de conteneurs en
  marche, **chaque conteneur avec Démarrer / Redémarrer / Arrêter**, ouvrir Solon, démarrer ou arrêter le
  moteur, quitter. Dans la langue de l'interface. Fermer la fenêtre laisse Solon actif dans la barre des tâches.

CLI `docker` existant :

```powershell
docker context create solon --docker host=npipe:////./pipe/solon
docker context use solon
```

> Si Docker Desktop est installé, le CLI utilise le gestionnaire d'identifiants Windows et peut envoyer
> des identifiants Docker Hub périmés (« unauthorized: incorrect username or password »). Faites
> `docker logout` ou utilisez un `DOCKER_CONFIG` vide pour le vérifier ; ce n'est pas lié à Solon.

## Performance des fichiers partagés : à savoir

Les dossiers Windows sont partagés par le protocole 9P de Windows (le même que `/mnt/c` sous WSL2). Le
débit séquentiel est bon (300–450 Mio/s) mais **chaque opération sur les métadonnées coûte 1 à 2 ms**
(`stat`, création de fichier). Un `npm install` ou un `git status` sur un gros projet monté depuis
Windows sera lent. Pour les dépendances et les bases de données, utilisez des **volumes Docker** :
ils vivent sur le disque de Solon, à vitesse native. Détails et mesures : `docs/measurements.md`.

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
- **Sortie de Compose** affichée à la fin de la commande, pas en flux ; un `docker compose up` long paraît figé
  jusqu'à la fin. Utilisez le CLI `docker compose` dans un terminal si vous voulez suivre en direct.
- **Ports UDP** publiés non relayés vers `localhost` (TCP seulement).
- **Un seul moteur par machine**, pas de profils multiples.
- **Montages de dossiers Windows** : métadonnées lentes (voir plus haut). Un dossier n'est partagé avec le
  moteur qu'à l'ouverture d'un projet Compose ; les chemins `C:\...` passés à `docker run -v` depuis le CLI ne
  sont pas encore traduits (le lecteur partagé est visible sous `/mnt/host/c/...`).
- **Docker Hub** : le CLI `docker` de Windows peut réutiliser des identifiants périmés stockés par Docker
  Desktop (voir « Utilisation »).
- **Installeur non signé** (avertissement SmartScreen) ; le chemin « activation de Hyper-V puis redémarrage »
  n'a pas encore été éprouvé sur une machine vierge.
- Pas de mise à jour automatique (aucune requête réseau) : installer la nouvelle version par-dessus.

## Documentation

- `ARCHITECTURE.md` : choix techniques (virtualisation HCS, image Linux, HvSocket, 9P, réseau), résultats
  mesurés bloc par bloc, registre des risques.
- `docs/measurements.md` : toutes les mesures et les faits établis.
- `tests/e2e/` : scénarios de bout en bout.
- `CONTRIBUTING.md` : comment contribuer.

## Licence

Apache 2.0, voir `LICENSE`.
