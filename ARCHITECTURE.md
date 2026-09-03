# Solon — Plan d'architecture

> **Statut : v0.1 validée le 2 septembre 2026 (six décisions de la section 0 acceptées). Le document sera complété par les mesures au fil des blocs.**
> Ce document deviendra la documentation technique de référence (`ARCHITECTURE.md`) une fois validé.

Solon est un gestionnaire de conteneurs autonome pour Windows, dans l'esprit d'OrbStack : une VM Linux minuscule, invisible, entièrement gérée par nous, avec des chemins de communication optimisés. Ce plan fixe les choix de virtualisation, de construction de l'image Linux, de communication hôte↔VM, de partage de fichiers, et le découpage du code. Il liste aussi, sans les cacher, les points pour lesquels il n'existe pas aujourd'hui de solution mature sur Windows.

---

## 0. Résumé exécutif et décisions à trancher

### Ce que je recommande

| Domaine | Recommandation MVP | Alternative étudiée | Confiance |
|---|---|---|---|
| Virtualisation | **HCS (Host Compute System API)** via `computecore.dll`, VM Hyper-V « utilitaire » en boot direct du noyau | OpenVMM/WHP (Rust, virtiofs) ; VMM maison sur WHP ; `wsl.exe` ; QEMU | Haute (précédents : WSL2, Cowork, Docker Desktop Hyper-V) |
| Noyau Linux | **Noyau `microsoft/WSL2-Linux-Kernel`** compilé par nous, config dérivée de la config WSL | `linux-virt` d'Alpine (binaire prêt, mais bug réseau Hyper-V signalé) | Haute |
| Système de fichiers racine | **Alpine Linux** minirootfs + `docker-engine`/`containerd`/`runc`/`docker-cli-compose`, dans un **VHDX en lecture seule** ; initrd minuscule ; **VHDX de données** ext4 séparé | Rootfs entier en initramfs (coûte ~200 Mo de RAM) | Haute |
| Canal de contrôle | **HvSocket** (`AF_HYPERV` côté Windows, `AF_VSOCK`/`hv_sock` côté Linux), toutes connexions initiées par l'hôte | Réseau TCP virtuel | Haute |
| Accès API Docker | Le service Windows relaie l'API Docker de HvSocket vers un **named pipe** `\\.\pipe\solon` ; `bollard` s'y connecte ; bonus : compatible CLI `docker` | Transport `bollard` custom directement sur HvSocket (gardé en option) | Haute |
| Partage de fichiers | **Plan9 (9P) natif de HCS**, monté à la demande par lecteur (`/mnt/host/c/...`) | virtiofs (**indisponible sur HCS**), sync type Mutagen, serveur 9P/FUSE maison | **Moyenne : fonctionne, mais performances de type WSL2 `/mnt/c`** |
| Réseau sortant (pull d'images) | **Réseau NAT HNS** (Host Network Service), comme WSL2 | Pile réseau en espace utilisateur sur HvSocket (type gvproxy) | Moyenne (conflits VPN d'entreprise connus) |
| Ports publiés (`localhost:8080`) | **Relais TCP sur HvSocket** dans le service Windows, piloté par les événements Docker | Port-forwarding HNS | Haute |
| Privilèges | **Service Windows `SolonService`** (LocalSystem) possède la VM ; l'app Tauri lui parle par named pipe ; l'installeur fait toute l'élévation | App élevée en permanence (UAC à chaque lancement) | Haute |
| Distribution de l'image Linux | **Embarquée dans l'installeur** (~100 Mo compressés estimés) | Téléchargée au premier lancement | Haute |
| Éditions Windows | **Windows 11 (et 10 22H2) Pro / Entreprise / Éducation** pour le MVP | Windows Home : nécessite un travail supplémentaire (voir §13) | — |

### Points sans solution mature aujourd'hui (je ne les tranche pas seul)

1. **virtiofs n'existe pas côté hôte Windows dans la pile Hyper-V/HCS.** Le seul chemin virtiofs sur Windows est OpenVMM (backend WHP), dont les interfaces « continuent d'évoluer et peuvent changer entre les versions ». Le partage 9P de HCS est la même technologie que `/mnt/c` sous WSL2 : correct pour des fichiers sources, lent pour `node_modules`, `git status` sur gros dépôts, bases de données. **Je propose 9P pour le MVP, derrière une interface remplaçable, avec mesures publiées.** Décision à confirmer (§5).
2. **Windows Home.** Les preuves de terrain (Cowork) montrent qu'une VM HCS tierce démarre avec la seule « Plateforme de machine virtuelle », mais que le partage Plan9 échoue sans le service `vmms` (Hyper-V complet, indisponible sur Home). **Je propose d'exiger Pro/Entreprise/Éducation pour le MVP** et de rendre Home possible en phase 2 via un serveur de fichiers maison sur HvSocket (§13). Décision à confirmer.
3. **Sauvegarde d'état de la VM (suspend/restore).** HCS le supporte en théorie pour les VM, mais ce n'est ni documenté pour les VM Linux tierces ni fiable pour reprendre `dockerd` avec des connexions ouvertes. **Je ne compte pas dessus** : le démarrage à froid d'un noyau en boot direct est de l'ordre de la seconde ; je mesurerai et annoncerai le chiffre.
4. **Réseau et VPN d'entreprise.** Le NAT HNS (approche WSL2) est connu pour casser sous certains clients VPN (AnyConnect, GlobalProtect) et pour entrer en conflit de plages IP avec Docker Desktop. Une pile réseau en espace utilisateur sur HvSocket (ce que fait podman avec gvproxy) évite ces problèmes mais représente un chantier notable en Rust. **Je propose HNS pour le MVP et la pile utilisateur comme premier chantier post-MVP** (§6).
5. **Cible mémoire < 500 Mo.** Plausible (WSL2 tient une VM système en ~150–250 Mo), mais dépend du mécanisme de reprise mémoire (`hv_balloon` + page reporting côté noyau, hints HCS côté hôte) que je ne pourrai confirmer qu'en mesurant.
6. **Signature de code.** Réduire les alertes SmartScreen exige un certificat OV/EV acheté ou Azure Trusted Signing (validation d'identité de l'organisation). Je peux câbler la chaîne de signature, pas obtenir le certificat.

### Questions pour toi (réponses attendues avant le code)

- **Q1** — Acceptes-tu « Windows Pro/Entreprise/Éducation requis » pour le MVP ?
- **Q2** — Acceptes-tu le partage 9P de HCS pour le MVP, avec ses limites de performance documentées ?
- **Q3** — Acceptes-tu le réseau NAT HNS pour le MVP (pile utilisateur en post-MVP) ?
- **Q4** — Acceptes-tu l'architecture « service Windows + app Tauri » plutôt qu'une application élevée ?
- **Q5** — Acceptes-tu que l'image Linux soit embarquée dans l'installeur (~100 Mo) ?
- **Q6** — Le noyau : compilation du noyau WSL2 par nous (recommandé) ou `linux-virt` d'Alpine ?

---

## 1. Contexte vérifié le 2 septembre 2026

### 1.1 Machine de test

| Élément | Valeur relevée |
|---|---|
| OS | Windows 11 Professionnel, build 26200 |
| CPU / RAM | Intel Core i7-13700 (16 cœurs), 13,7 Go |
| Hyperviseur | `HypervisorPresent = True` (Hyper-V déjà actif) |
| Fonctionnalités activées | `Microsoft-Hyper-V-All`, `VirtualMachinePlatform`, `Microsoft-Windows-Subsystem-Linux` |
| Fonctionnalités désactivées | `HypervisorPlatform` (WHP), `Containers`, `Containers-HNS` |
| Déjà installés | WSL2 (Ubuntu), CLI Docker 29.3.1 (Docker Desktop) |
| DLL présentes | `computecore.dll`, `computenetwork.dll`, `WinHvPlatform.dll`, `virtdisk.dll` |
| Outils | Rust 1.94, Node 22.18, npm 10.9 |

Conséquences : (a) tous les prérequis de la piste HCS sont déjà réunis ici, le spike peut démarrer immédiatement ; (b) Docker Desktop cohabite : Solon ne doit jamais toucher au pipe `docker_engine`, ni aux réseaux HNS de Docker Desktop, ni au contexte `docker` par défaut de l'utilisateur ; (c) la machine ne permet **pas** de tester le cas Windows Home ni le cas « fonctionnalités désactivées » sans manipulation volontaire (à faire dans une VM Hyper-V imbriquée ou sur une seconde machine).

### 1.2 État de l'art (précédents exploitables)

- **WSL2** : VM utilitaire HCS, noyau `microsoft/WSL2-Linux-Kernel` en boot direct, disques VHDX, HvSocket, 9P pour `/mnt/c`, réseau NAT HNS, service Windows (`wslservice`). C'est l'architecture la plus proche de ce que nous voulons, et elle fonctionne avec la seule « Plateforme de machine virtuelle ».
- **Claude Cowork (Anthropic, 2026)** : configuration HCS observée publiquement : `Chipset.LinuxKernelDirect` (`vmlinuz` + `initrd` + `root=/dev/sda1`), disques SCSI VHDX (`rootfs.vhdx`, données), partage Plan9 (`path=C:\Users\... port=9999 flags=0x10`), HvSocket avec GUID de services, `VirtioSerial` pour la console, 4 Go avec surallocation, service Windows `CoworkVMService`. Exige Windows 11 Pro/Entreprise ; erreur `Plan9 mount failed: bad address` sur Home. Bugs de terrain instructifs : service laissé en démarrage « Manuel », conflit de plage IP fixe `172.16.0.0/24` avec VPN et Docker, déplacement de VHDX entre lecteurs.
- **Docker Desktop** : historiquement Hyper-V (Samba puis gRPC-FUSE) et WSL2 (9P). Depuis la 4.86 (août 2026), Docker VMM, hyperviseur maison, en bêta sur Windows, sans détail public sur l'API utilisée. Confirme la direction « VMM dédié aux conteneurs » mais ne fournit rien de réutilisable.
- **podman machine (Hyper-V)** : réseau via gvproxy en espace utilisateur sur HvSocket, partages 9P, enregistrement des GUID HvSocket dans le registre par `podman-system-hyperv-prep` (élévation requise). Prouve la viabilité du réseau sur HvSocket.
- **OpenVMM (Microsoft, Rust, MIT)** : VMM modulaire, backends WHP (Windows), KVM/MSHV (Linux), Hypervisor.framework (macOS). Périphériques : virtio-fs (hôtes Linux **et Windows**), virtio-9p, virtio-vsock, virtio-net avec NAT « Consomme », disques VHD/VHDX, boot direct Linux, enlightenments Hyper-V/VMBus. Avertissement officiel : les interfaces de gestion « peuvent changer entre les versions ». C'est le seul chemin virtiofs sur Windows ; c'est un candidat sérieux pour une phase ultérieure, pas pour un MVP à livrer.

### 1.3 Résultats du bloc 0a (spike VM, 2 septembre 2026)

Détail et tableau complet dans `docs/measurements.md`. En résumé :

- Une VM HCS en boot direct du noyau démarre depuis Rust (`solon-vm-hcs`, bindings `windows` 0.62) : création 51 ms, `HcsStartComputeSystem` 29 ms, **espace utilisateur atteint ~0,7 s** après l'ordre de démarrage, arrêt propre détecté par l'événement `HcsEventSystemExited` avec un document JSON (`ExitType`, `Initiator`) qui distingue arrêt propre et crash.
- Les champs HCS retenus (`SchemaVersion 2.2`, `LinuxKernelDirect`, `ComPorts`, `HvSocket`, `AllowOvercommit`, `EnableDeferredCommit`, `ShouldTerminateOnLastHandleClosed=false`) sont acceptés sur Windows 11 26200.
- Le pipe de console est servi par `vmwp` ; le client se connecte dès le retour de `HcsStartComputeSystem`.
- Sans élévation : `HCS_E_ACCESS_DENIED` immédiat, message Windows explicite. **Le service Windows est confirmé.**
- Le noyau WSL2 a en dur tout ce dont Solon a besoin (hv_sock, 9P, balloon, page reporting, squashfs, overlay) sauf `BRIDGE` et `EROFS` (modules) : notre configuration les passera en `=y`. Le noyau Alpine `linux-virt` démarre aussi mais ses pilotes Hyper-V sont en modules : plan B confirmé, pas mieux.

### 1.4 Résultats du bloc 0b (canal HvSocket et partage 9P, 2 septembre 2026)

- **HvSocket** : connexion en 1 ms, aller-retour RPC ~450 µs (p99 < 900 µs), débit 2,3–2,7 Gio/s invité→hôte et 3,5–5 Gio/s hôte→invité. `tokio::net::TcpStream::from_std` accepte le socket : le service sera entièrement asynchrone. **Go** pour HvSocket et Tokio.
- **9P (Plan9 HCS)** : débit séquentiel 280–450 Mio/s ; **métadonnées ~1,2–1,9 ms par opération** (`stat`, `create`, `open`), soit la classe de performance de `/mnt/c` sous WSL2. `msize` plus grand n'apporte rien ; `cache=loose` divise le coût des métadonnées par ~2,5 mais casse la cohérence. **Go conditionnel** : 9P pour le MVP comme prévu, avec le risque R1 confirmé par la mesure et l'UI qui oriente vers les volumes pour les dépendances et bases de données.
- Un partage n'accepte **qu'une session 9P** (second montage : `EFAULT`) ; ajout/retrait à chaud de partages validés ; `ReadOnly` et `LinuxMetadata` (chmod conservé) validés ; écritures visibles instantanément des deux côtés.
- L'agent invité (Rust, musl statique) se compile croisé depuis Windows avec `rust-lld`, sans chaîne C.

### 1.5 Résultats du bloc 1 (image complète et moteur Docker, 2 septembre 2026)

- Pipeline `image/build.sh` opérationnel : noyau `6.18.40.1-solon` compilé depuis le dépôt WSL2 (fragment `solon.config`, familles netfilter en dur), rootfs Alpine v3.24 avec Docker Engine 29.5.3 en VHD fixe de 293 Mo, initrd de 0,6 Mo, manifeste avec SHA-256.
- **Moteur Docker prêt ~1,2 s après l'ordre de démarrage** (initrd 0,69 s, agent PID 1 0,76 s, dockerd 1,16 s), `docker version` depuis le CLI Windows via `\\.\pipe\solon` en 125 ms, `docker run --rm` en ~620 ms. Persistance du disque de données validée sur deux cycles, arrêt propre en ~0,4 s.
- Quatre pièges levés et documentés dans `docs/measurements.md` : ACL du groupe Virtual Machines sur les disques, disposition du pied de VHD, environnement vide de PID 1, options netfilter en modules dans la configuration WSL, et absence de demi-fermeture des named pipes (relais réécrit).

### 1.6 Résultats du bloc 2 (service Windows, 2 septembre 2026)

- **Service `solon-service` opérationnel** (mode service Windows et mode console) : machine à états complète (§7.1), prérequis, image vérifiée par SHA-256, disque de données VHDX créé par `CreateVirtualDisk`, détection d'orphelines et rattachement, réseau HNS, machine, agent, configuration réseau de l'invité, attente de dockerd. Canal de contrôle `\\.\pipe\solon-control` et API Docker `\\.\pipe\solon` accessibles **sans élévation**.
- **Réseau sortant validé** : réseau HNS de type ICS (comme WSL2), adresse statique poussée à l'agent, `docker pull` depuis un registre public en 0,9 s ; DNS de l'hôte relayés.
- **Ports publiés** : détection dans l'invité par le flux d'événements de dockerd, relais TCP hôte → HvSocket → conteneur ; `http://localhost:8080` répond depuis Windows.
- **Robustesse** : terminaison brutale de la machine pendant des écritures → redémarrage, `fsck` propre, données synchronisées conservées, perte non synchronisée bornée à ~2 s par un `sync()` périodique de l'agent. Service tué pendant que la machine tourne → relance et **rattachement** à la même machine, conteneurs intacts.
- **RAM au repos : 426–516 Mo** (machine + service) pour 2 048 Mo alloués : la cible de 500 Mo est atteinte sans marge ; leviers pour le bloc de durcissement : `drop_caches` après inactivité, hints mémoire HCS, allocation initiale adaptée à la RAM de l'hôte.
- **Temps de démarrage mesuré (build release) : 2,5–2,6 s** de l'ordre de démarrage au moteur Docker prêt, tout compris (prérequis, SHA-256 de l'image, réseau HNS, machine, agent, dockerd).
- Trois découvertes documentées dans `docs/measurements.md` : `connect()` HvSocket bloqué 30 s sans `HVSOCKET_CONNECT_TIMEOUT` ; le CLI `docker events` ne vide pas sa sortie redirigée (lecture directe de `GET /events`) ; le CLI `docker` de Windows envoie des identifiants du gestionnaire d'identifiants Windows quand Docker Desktop est installé.

### 1.7 Résultats du bloc 3 (application de bureau, 2 septembre 2026)

- **Application Tauri v2 fonctionnelle** contre le vrai service : barre d'état alimentée par l'abonnement au service, écran de première installation avec les étapes en direct et les erreurs traduites par code, vue conteneurs temps réel (filtre, groupes Compose, CPU/RAM en flux), actions avec confirmation, journaux en flux, terminal `xterm.js` bidirectionnel avec redimensionnement, inspection, réglages, thème sombre/clair, **anglais par défaut et français**.
- Aucune élévation dans l'application : tout passe par les deux named pipes du service ; le client réessaie sur `ERROR_PIPE_BUSY` (découverte du bloc).
- Fonctionnalités MVP couvertes à ce stade : 1 (provisionnement), 2 (vue conteneurs), 3 (actions, logs, terminal). Restent 4, 5, 6, 7 pour le bloc 4.

### 1.8 Résultats du bloc 4 (images, volumes, réseaux, Compose, barre des tâches, 2 septembre 2026)

- **Les 7 fonctionnalités du MVP sont présentes** : vues Images (lancer, inspecter, supprimer), Volumes et Réseaux (créer, inspecter, supprimer), Compose (dossier Windows partagé à la demande par 9P, `docker compose` exécuté dans la machine via l'agent, regroupement par projet), icône de barre des tâches (état, conteneurs en marche, démarrer/arrêter, ouvrir, quitter).
- Le partage de dossier à la demande implémente la §5.2 : un partage par lecteur, monté sur `/mnt/host/<lettre>`, chemin traduit ; découverte : le périphérique Plan9 doit être déclaré à la création de la machine pour accepter des ajouts à chaud.
- Limites assumées : sortie Compose capturée en fin de commande (pas de flux), UDP non relayé, une seule machine par hôte.

### 1.9 Résultats du bloc 5 (durcissement, 2 septembre 2026)

- **Mémoire** : hints HCS `EnableHotHint/EnableColdHint/EnableColdDiscardHint` activés (c'est ce que fait WSL2 pour rendre la mémoire libérée par l'invité) ; l'agent relâche le cache de pages (`drop_caches`) toutes les 2 min quand la charge est basse. Mesure au repos : voir `docs/measurements.md` (bloc 5).
- **Réseau** : MTU 1400 côté invité et côté conteneurs (`daemon.json`), pour survivre aux VPN qui encapsulent.
- **Sécurité** : pipes `\.\pipe\solon` et `solon-control` accessibles aux utilisateurs **interactifs** (IU) et administrateurs seulement, plus au groupe « utilisateurs authentifiés » ; le service ne fait toujours aucune requête sortante.
- **Réglages** appliqués au prochain démarrage du moteur sans redémarrer le service.
- **Interface** : écran de démarrage avec le détail des prérequis (état, explication, action) ; contraste AA du texte secondaire relevé ; test `locales.rs` garantissant qu'aucun code d'erreur ni prérequis n'est sans traduction et que `en.json`/`fr.json` ont les mêmes clés.
- **Installeur** : image et service installés à côté de l'exécutable (`<install>\image`, `<install>\solon-service.exe`), le service la trouve sans copie ; `installer/setup.ps1` (élevé) active les composants Windows, enregistre les GUID HvSocket et installe `SolonService` ; désinstallation avec question avant de supprimer `%ProgramData%\Solon`.
- **Installeur testé de bout en bout** (`tests/e2e/install-test.ps1`) : installation silencieuse en 40 s, service Windows réel (LocalSystem, session 0) qui démarre le moteur en 4 s au premier lancement, `docker` accessible sans élévation, port publié relayé, arrêt propre.
- Mesure honnête : le plancher mémoire au repos reste **426 Mo** malgré les hints (ballon et signalement de pages libres actifs dans l'invité) ; voir `docs/measurements.md` pour l'analyse et les pistes.
- Reste fragile : pas encore testé sur une machine où Hyper-V est **désactivé** (chemin « activation + redémarrage » de `setup.ps1`) ni sur Windows Famille ; installeur non signé (SmartScreen).

### 1.10 Résultats du bloc 6 (livraison, 3 septembre 2026)

- Livrables : `README.md` (prérequis, installation, dépannage par code), `CONTRIBUTING.md`, `LICENSE` (Apache-2.0), ce document, `docs/measurements.md`, installeur NSIS `Solon_0.1.0_x64-setup.exe` (80 Mo) construit et testé (§1.9), chaîne de signature prête mais non exercée (pas de certificat).
- Direction visuelle revue d'après la maquette de l'utilisateur (3 septembre 2026) : **thème clair par défaut** (réglage Apparence : clair / sombre / suivre Windows, barre de titre native synchronisée), statut des conteneurs en point coloré (texte en infobulle et `aria-label`), actions de ligne en boutons icône, « Ouvrir un projet… » en bouton principal ; logo « cube bleu » façon OrbStack (SVG vectoriel maison, pps/desktop/src-tauri/icons/src/icon.svg), accent bleu tiré du logo, barre latérale calme avec icônes, coins arrondis 8/12 px, thèmes clair et sombre retravaillés (jetons dans styles.css).
- Barre des tâches traduite (anglais/français, suit la langue de l'interface, libellés lus dans les mêmes fichiers de langue que le frontend) ; à la demande de l'utilisateur, un clic sur l'icône ouvre un menu natif listant les conteneurs (en marche d'abord, 12 au plus) avec Démarrer / Redémarrer / Arrêter par conteneur, menu reconstruit toutes les 3 s si la liste change ; icône définitive (colonne, référence au législateur athénien) générée pour toutes les tailles.
- **Lot du 3 septembre 2026 (soir)** : (1) **vue par projet Compose** (liste récents + détectés via l'étiquette `com.docker.compose.project.working_dir` traduite en chemin Windows ; écran par projet : services, journaux mêlés — un flux `logs` par conteneur préfixé du nom de service —, Up / Down / Rebuild, Explorateur, VS Code via `code` puis `vscode://`) ; (2) **terminal dans la machine** : l'agent écoute le vsock **5004**, ouvre un pseudo-terminal (`openpty`) et lance `/bin/sh -l` ; hôte → agent : en-tête JSON puis trames `[type, len16, charge]` (saisie / redimensionnement) ; agent → hôte : octets bruts ; le service relaie le pipe `\\.\pipe\solon-shell` ; l'application le présente dans une modale xterm.js (`Ctrl+\``) ; (3) **CLI intégré** : `crates/solon-docker-shim` produit `bin\docker.exe`, lanceur du CLI Docker officiel (`docker-cli.exe`, téléchargé par `installer/fetch-docker-cli.ps1`, redistribué sous Apache-2.0) avec `DOCKER_HOST` Solon par défaut et `DOCKER_CLI_PLUGIN_EXTRA_DIRS` vers le plugin Compose livré ; `setup.ps1` ajoute `<install>\bin` en tête du PATH machine (retiré à la désinstallation) ; (4) **recherche globale Ctrl+K** (palette : sections, actions moteur, conteneurs, images, volumes, réseaux, projets ; `Ctrl+1…6`) ; (5) **processeurs par défaut = cœurs logiques − 2** (au moins 2), calculé par le service quand `settings.json` est absent.
- Limites connues consignées dans le README : sortie Compose non diffusée en flux, UDP non relayé, une machine par hôte, Windows Famille exclu, identifiants Docker Hub du CLI hérités de Docker Desktop.

---

## 2. Virtualisation : choix et justification

### 2.1 Options comparées

| Option | Éditions Windows | Périphériques disponibles | Maturité | Coût de développement | Verdict |
|---|---|---|---|---|---|
| **HCS** (`computecore.dll`, schéma JSON v2) | Pro+ en pratique pour Plan9 ; VM seule probablement OK sur Home | SCSI/VHDX, HvSocket, Plan9, VirtualSmb, NIC HNS, série/VirtioSerial, mémoire dynamique avec hints | Production (WSL2, Sandbox, Docker Desktop, Cowork) | Faible : la VM et ses périphériques sont fournis par Windows | **Retenu pour le MVP** |
| **OpenVMM sur WHP** | Toutes (WHP existe sur Home) | virtio-fs, 9p, vsock, net NAT, VHDX, boot direct | Interfaces instables ; pas de garantie de support | Moyen à élevé : intégrer un dépôt volumineux, suivre ses changements, packager `openvmm.exe` ou lier ses crates | **Phase 2 candidate** (virtiofs + Home) |
| **VMM maison sur WHP** (`WinHvPlatform.dll`) | Toutes | Tout à écrire : PCI/MMIO, virtio-blk/net/vsock/fs, interruptions, chargeur | — | Très élevé (Firecracker ≈ 50 k lignes) | Écarté |
| **`wsl.exe`** | Toutes | Tout de WSL2 | Production | Faible | Écarté : contredit le cahier des charges (contrôle total, pas de dépendance à WSL) |
| **QEMU + accélérateur WHPX** | Toutes | Tout virtio, y compris virtiofsd (Linux seulement côté hôte : pas de virtiofsd Windows) | Production | Faible, mais binaire lourd, lent au démarrage, pas « notre » VM | Écarté |

### 2.2 Ce que HCS implique concrètement

- **Bindings** : `windows-rs` expose `Win32::System::HostComputeSystem` (HCS), `Win32::System::HostComputeNetwork` (HNS), `Win32::Storage::Vhd` (virtdisk) et `AF_HYPERV`/`SOCKADDR_HV` (WinSock). Pas de crate Rust de haut niveau : nous écrivons `solon-vm-hcs` nous-mêmes, en s'inspirant de `hcsshim` (Go) pour le schéma et les séquences.
- **Document de VM (schéma v2.x)** : `Owner="Solon"`, `SchemaVersion 2.x`, `VirtualMachine.Chipset.LinuxKernelDirect{KernelFilePath, InitRdPath, KernelCmdLine}`, `ComputeTopology.Memory{SizeInMB, AllowOvercommit, EnableDeferredCommit, EnableHotHint, EnableColdHint, EnableColdDiscardHint}`, `ComputeTopology.Processor{Count}`, `Devices.Scsi["0"].Attachments{0: rootfs.vhdx (ReadOnly), 1: data.vhdx}`, `Devices.HvSocket.HvSocketConfig.ServiceTable{GUID → descripteurs de sécurité}`, `Devices.Plan9.Shares[]`, `Devices.NetworkAdapters{EndpointId HNS}`, `Devices.ComPorts["0"].NamedPipe` (console série pour le diagnostic). Les champs exacts sont validés au spike (§12, bloc 0).
- **Cycle de vie** : un « compute system » HCS est **éphémère** : arrêté, il disparaît. Chaque démarrage recrée la VM à partir du document JSON : rien d'ancien ne peut rester incohérent côté HCS. `HcsOpenComputeSystem(id)` permet au service, s'il a redémarré, de **se rattacher** à une VM encore en marche. `HcsEnumerateComputeSystems` avec un filtre sur `Owner` permet de détecter et de terminer une VM orpheline.
- **Privilèges** : la création de compute systems exige Administrateur ou groupe « Hyper-V Administrators ». D'où le **service Windows** `SolonService` (LocalSystem), installé par l'installeur, qui possède la VM, le relais Docker, les relais de ports et les montages. L'application Tauri, non élevée, lui parle par un named pipe protégé par ACL (utilisateur interactif). Le service ne fait **aucune** requête réseau sortante.
- **Prérequis système** : fonctionnalités `Microsoft-Hyper-V` (Pro+) et `VirtualMachinePlatform`, virtualisation matérielle active, services `vmcompute` et `hns` démarrés. Le tout activé par l'installeur (DISM, élévation, redémarrage), et revérifié au démarrage du service (§7).

---

## 3. Image Linux minimale

### 3.1 Composition (trois fichiers, versionnés ensemble)

```
image/<version>/
├── vmlinuz           noyau Linux, bzImage (15 Mo mesurés, 6.18.40.1-solon)
├── initrd.img        initramfs minimal : busybox + script init (0,6 Mo mesurés)
├── rootfs.vhd        système racine en LECTURE SEULE, ext4 sans journal dans un VHD fixe (293 Mo mesurés)
└── manifest.json     versions, SHA-256 des trois fichiers, options noyau, date de build
```

Choix du bloc 1 : le disque racine est un **VHD à taille fixe** (données brutes + pied de 512 octets) et non un VHDX. Le format se génère avec 60 lignes de Python (`image/tools/mkvhd.py`), sans `qemu-img` ni outil Windows, et HCS l'accepte. Comme il est en lecture seule et compressé dans l'installeur, sa taille pleine est sans conséquence. Le disque de données, lui, est un **VHDX dynamique** créé par Windows (`CreateVirtualDisk`) au premier lancement. L'agent (`/sbin/solon-agent`) vit dans le rootfs, pas dans l'initrd : l'initrd ne fait que monter le rootfs sous une surcouche `overlay` en tmpfs et `switch_root`.

Au premier démarrage, le service crée en plus `data.vhdx` (VHDX dynamique, 64 Go max par défaut) que l'agent formate en ext4 dans la VM. Il contient `/var/lib/docker`, `/var/lib/containerd`, l'état de l'agent, et il **survit aux mises à jour de l'image** : mettre Solon à jour remplace `image/<version>/`, jamais `data.vhdx`.

Pourquoi un rootfs en VHDX et non en initramfs : un initramfs vit en RAM (tmpfs). Avec `dockerd` + `containerd` + `runc` + compose + CLI (~200 Mo décompressés), il consommerait à lui seul près de la moitié du budget mémoire. Un VHDX en lecture seule n'occupe que le cache de pages, récupérable par le ballon.

### 3.2 Contenu du rootfs

Base : **Alpine Linux** (version épinglée, `alpine-minirootfs`). Paquets : `docker-engine`, `docker-cli`, `docker-cli-compose`, `containerd`, `runc`, `iptables`/`nftables`, `e2fsprogs`, `ca-certificates`, `busybox`, et notre binaire `solon-agent` (Rust, cible `x86_64-unknown-linux-musl`, statique). Retirés : `apk` lui-même après build, docs, locales, `openrc` (l'agent est PID 1), tout service réseau (`sshd`, `chrony` : l'horloge est fournie par `hv_utils`).

L'agent est **PID 1** : il monte les systèmes de fichiers, lance `containerd` puis `dockerd` (`-H unix:///run/docker.sock`, `--live-restore=false`, `--userland-proxy=false`), surveille les processus, répond aux RPC de l'hôte, et orchestre l'arrêt propre.

### 3.3 Noyau

Recommandation : **`microsoft/WSL2-Linux-Kernel`** (GPL-2.0), compilé par nous à partir d'un tag épinglé, avec la configuration WSL comme base et un fragment `solon.config` qui retire ce qui est inutile (`dxgkrnl`, USB, son…) et garantit : `HYPERV` (VMBus, stockage `hv_storvsc`, réseau `hv_netvsc`), `HYPERV_VSOCKETS` (`hv_sock`), `HYPERV_UTILS` (horloge, arrêt), `HYPERV_BALLOON` + `PAGE_REPORTING` (reprise mémoire), `NET_9P` + `9P_FS` + `9P_FS_POSIX_ACL` + `9P_FS_SECURITY`, `EXT4_FS`, `OVERLAY_FS`, `CGROUPS` v2 complets, `NETFILTER`/`NF_TABLES`/`BRIDGE`/`VETH`/`IP_NF_*`, `VIRTIO_CONSOLE`. Tout en `=y` : pas de modules à charger, pas de `modprobe` dans l'initrd. Vérifié au bloc 0a : la configuration WSL2 publiée a déjà tout cela en dur, à l'exception de `BRIDGE` et `EROFS_FS` (modules), que notre fragment force en `=y`.

Pourquoi pas `linux-virt` d'Alpine : binaire prêt et petit, mais un problème réseau sous Hyper-V a été signalé sur 3.19 (`aports#16096`) et sa configuration n'est pas pilotée par nous. Il reste l'option de secours du spike si la compilation du noyau prend trop de temps.

### 3.4 Construction reproductible

Pipeline `image/` exécuté **dans un conteneur Linux** (Docker ou WSL2 sur la machine de dev, GitHub Actions `ubuntu-latest` en CI) :

1. `Dockerfile.kernel` : clone du tag, application de `solon.config`, `make -j bzImage`, sortie `vmlinuz`.
2. `Dockerfile.rootfs` : `apk --root ... add` des paquets épinglés par version exacte (`APKINDEX` figé), copie de `solon-agent`, nettoyage, `mkfs.ext4 -d` d'une image brute, conversion en VHDX par `qemu-img convert -O vhdx` (ou squashfs si le gain le justifie, décidé au spike).
3. `initrd` : `cpio` + `zstd`, `SOURCE_DATE_EPOCH` fixé pour un résultat identique à chaque build.
4. `manifest.json` signé par les SHA-256 ; version sémantique de l'image indépendante de celle de l'app, avec une matrice de compatibilité minimale dans le service.

Engagement de maintenance : une nouvelle image à chaque CVE noyau ou Docker notable ; la CI reconstruit les trois fichiers, le test d'intégration (§10) démarre la VM et exécute `docker run hello-world` (image pré-chargée localement en CI, pas de réseau).

---

## 4. Canal hôte↔VM : HvSocket

### 4.1 Mécanique

- Côté Windows : socket `AF_HYPERV` (famille 34), `SOCK_STREAM`, protocole `HV_PROTOCOL_RAW`, adresse `SOCKADDR_HV{VmId, ServiceId}`. `VmId` = identifiant de la VM HCS. Pour un invité Linux, le `ServiceId` encode le port vsock : `xxxxxxxx-facb-11e6-bd58-64006a7986d3` où `xxxxxxxx` est le port en hexadécimal.
- Côté Linux : `AF_VSOCK` avec transport `hv_sock`, CID hôte = 2. L'agent écoute sur des ports fixes.
- **Toutes les connexions sont initiées par l'hôte** (service → agent). Cela évite d'enregistrer des services hôtes dans `HKLM\...\Virtualization\GuestCommunicationServices` pour l'écoute côté hôte (l'installeur enregistre quand même nos GUID pour garder la porte ouverte), et cela simplifie le modèle de sécurité : l'invité ne peut rien ouvrir vers l'hôte.
- Intégration Tokio : wrapper `HvStream` sur un socket brut `AF_HYPERV` enregistré auprès de `mio` via `FromRawSocket` (IOCP/AFD acceptent ces sockets, c'est ce que fait WSL). **À valider au spike** ; repli : threads bloquants dédiés, ce qui suffit largement pour le trafic de contrôle.

### 4.2 Ports et protocoles

| Port vsock | Rôle | Protocole |
|---|---|---|
| 5000 | RPC de contrôle agent | Trames `u32 longueur + JSON` (`serde`), requêtes/réponses avec identifiant, notifications asynchrones |
| 5001 | API Docker | Octets bruts relayés vers `/run/docker.sock` (HTTP/1.1, hijack inclus) |
| 5002 | Relais de ports publiés | Une connexion par flux TCP entrant ; en-tête initial `{port, container_ip}` puis octets bruts |
| 5003 | Flux Compose / exec agent | Une connexion par commande : stdout/stderr multiplexés en trames, code de retour final |
| 5004 | Console de diagnostic | Journal de l'agent en flux |

RPC de contrôle (liste initiale) : `Ping`, `GetHealth`, `MountShare{name, port, target, read_only}`, `UnmountShare`, `ListMounts`, `GetStatsSnapshot` (optionnel, voir §8.4), `ComposeRun{project_dir, args}`, `PrepareShutdown{timeout}`, `Poweroff`, `FsckReport`, `SetTimezone`.

### 4.3 Exposition de l'API Docker côté Windows

Le service ouvre `\\.\pipe\solon` (ACL : SYSTEM + utilisateur interactif) et relaie chaque connexion vers le port 5001 de la VM, y compris les connexions hijackées (`exec`/`attach`). Avantages : (1) `bollard` utilise `connect_with_named_pipe`, zéro code exotique dans l'app ; (2) l'utilisateur peut faire `docker context create solon --docker host=npipe:////./pipe/solon` avec son CLI existant ; (3) c'est le point unique où appliquer la **réécriture des chemins Windows** dans `POST /containers/create` (`Binds`, `Mounts`) : `C:\Users\v\proj` → `/mnt/host/c/Users/v/proj`, en déclenchant au passage le montage du lecteur s'il ne l'est pas. `bollard::Docker::connect_with_custom_transport` reste l'option si l'on veut supprimer un saut plus tard.

---

## 5. Partage de fichiers hôte↔VM

### 5.1 État de l'art sur Windows (vérifié)

| Technologie | Disponible avec HCS ? | Performance attendue | Remarques |
|---|---|---|---|
| **virtiofs** | **Non** (aucun périphérique hôte dans Hyper-V/HCS) | Excellente (référence OrbStack/macOS) | Seul chemin : OpenVMM/WHP, interfaces instables |
| **Plan9 (9P) HCS** | Oui (`Devices.Plan9.Shares`, drapeaux `ReadOnly 0x1`, `LinuxMetadata 0x4`, `CaseSensitive 0x8`, `RestrictFileAccess 0x10`) | Type WSL2 `/mnt/c` : correct en accès séquentiel, lent en métadonnées (stat massifs, `npm install`, `git status`) | Serveur 9P fourni par Windows ; requiert Hyper-V complet (`vmms`) d'après Cowork ; montage invité `mount -t 9p -o trans=fd,rfdno=N,wfdno=N,msize=65536,aname=<nom>` sur un fd HvSocket, `msize` max 64 Kio |
| **VirtualSmb / CIFS** | Oui | Médiocre ; conçu pour invités Windows | Écarté |
| **gRPC-FUSE / 9P maison sur HvSocket** | Oui (notre code) | Réglable (cache d'attributs agressif, notifications de changement côté Windows) | Chantier conséquent ; **débloque Windows Home** |
| **Synchronisation (type Mutagen, Docker « synchronized file shares »)** | Oui (notre code) | Native ext4 côté conteneur ; latence de sync 100 ms–1 s ; double stockage | Excellent pour `node_modules`, mauvais pour les gros fichiers binaires modifiés en place |
| **Volumes nommés dans `data.vhdx`** | Oui | Native | Toujours la voie rapide ; l'UI doit l'encourager (dépendances, BDD) |

### 5.2 Décision MVP et interface

- MVP : **Plan9 HCS**, ajouté et retiré à chaud par `HcsModifyComputeSystem` (`ResourcePath=VirtualMachine/Devices/Plan9/Shares`), un partage **par lecteur** (`C:`, `D:`…) monté à la demande sous `/mnt/host/<lettre>` avec le drapeau `LinuxMetadata` (permissions POSIX stockées en attributs étendus NTFS). Contrainte mesurée au bloc 0b : **une seule session 9P par partage** (le second montage échoue avec `EFAULT`), donc exactement un montage par lecteur ; `msize=65536` (les tailles supérieures n'apportent rien) ; pas de `cache=loose` par défaut (incohérence avec les modifications faites côté Windows). `AllowedFiles` et `RestrictFileAccess` sont évalués pour restreindre l'exposition aux dossiers réellement montés (à mesurer : coût d'un partage par dossier vs par lecteur).
- Interface Rust `HostShareProvider { fn expose(&self, host_path) -> Result<GuestPath>; fn release(...); fn stats(...) }` implémentée par `Plan9ShareProvider`. Les implémentations futures (`SyncShareProvider`, `SolonFsProvider`) ne changent ni l'agent ni l'UI.
- **Mesures publiées dans le README** dès le bloc 0 : débit séquentiel, `stat` massif, `npm install` d'un projet type, `git status` sur un dépôt de 50 k fichiers, comparés à WSL2 `/mnt/c` et à un volume nommé.
- Message honnête dans l'UI (sans mentionner de VM) : « Les fichiers Windows montés sont plus lents que les volumes Solon. Pour les dépendances et les bases de données, préférez un volume. »

---

## 6. Réseau

### 6.1 Ports publiés (entrant)

Le service écoute les événements Docker ; à chaque conteneur avec `HostConfig.PortBindings`, il ouvre un écouteur TCP sur l'hôte (`127.0.0.1` ou `0.0.0.0` selon la liaison demandée) et relaie chaque connexion vers le port 5002 de l'agent, qui la connecte à l'adresse du conteneur. `localhost:8080` fonctionne donc sans aucune dépendance au réseau virtuel, ni au pare-feu Windows pour les liaisons locales. UDP : hors MVP (documenté).

### 6.2 Sortant (pull d'images, trafic des conteneurs)

MVP : réseau **HNS de type NAT** créé par le service (`HcnCreateNetwork`, plage choisie **dynamiquement** pour éviter les collisions avec Docker Desktop, WSL, VPN, en scannant les routes de l'hôte), endpoint attaché à la VM, adresse invitée en DHCP HNS, DNS relayé vers les résolveurs de l'hôte. Problèmes connus et gérés : (1) conflits VPN : détection d'absence de connectivité depuis l'agent (`connect` TCP vers un résolveur, jamais de télémétrie) avec message actionnable ; (2) MTU des VPN (fixée à 1350 si VPN détecté) ; (3) la plage IP est recalculée à chaque démarrage si une collision apparaît.

Post-MVP : **pile réseau en espace utilisateur** (Rust, `smoltcp`) transportée sur HvSocket, à la manière de gvproxy : élimine la dépendance à HNS, supprime les conflits VPN, prépare Windows Home. Chantier isolé derrière `NetworkProvider`.

---

## 7. Cycle de vie, robustesse, prérequis

### 7.1 Machine à états du provisionnement (service)

```
Uninitialized
 → CheckingPrerequisites ──(manque)──→ PrerequisitesMissing{raisons, actions}
 → PreparingImage (vérif. SHA-256, copie vers %ProgramData%\Solon\image\<v>)
 → CreatingDataDisk (CreateVirtualDisk VHDX dynamique)
 → CreatingNetwork (HNS)
 → CreatingVm (HcsCreateComputeSystem) → Booting (HcsStartComputeSystem)
 → WaitingAgent (connexion port 5000, Ping, FsckReport)
 → WaitingEngine (GetHealth : dockerd répond sur /run/docker.sock)
 → Ready
 ⇢ Stopping → Stopped ; ⇢ Failed{code, message, action}
```

Chaque transition est journalisée (`tracing`, fichiers tournants dans `%ProgramData%\Solon\logs`) et diffusée à l'UI par événements. Chaque `Failed` porte un **code stable** documenté dans le README.

### 7.2 Détection des prérequis et messages actionnables

| Détection | Source | Message utilisateur (FR, sans « VM ») |
|---|---|---|
| Virtualisation matérielle désactivée | `Win32_ComputerSystem.HypervisorPresent=false` et `Win32_Processor.VirtualizationFirmwareEnabled=false` | « La virtualisation matérielle est désactivée dans le BIOS/UEFI. Activez « Intel VT-x » ou « AMD-V » (souvent dans l'onglet Advanced/CPU), puis relancez Solon. » |
| Édition Home | `Win32_OperatingSystem.OperatingSystemSKU` ∈ SKU « Core » | « Solon nécessite Windows 11 Pro, Entreprise ou Éducation. » + lien de documentation |
| Fonctionnalité manquante | API DISM (`DismGetFeatureInfo`) | « Un composant Windows doit être activé. Solon va le faire ; un redémarrage sera nécessaire. » (bouton) |
| Activation refusée par stratégie | HRESULT DISM `0x800f0954`, `0x800f0805`, `0x800f0922`, erreurs GPO | « Votre organisation restreint l'activation de composants Windows. Transmettez à votre administrateur : activer `Microsoft-Hyper-V` et `VirtualMachinePlatform`. » |
| Hyperviseur non lancé (autre hyperviseur, `hypervisorlaunchtype off`, Solon dans une VM sans virtualisation imbriquée) | HCS `HCS_E_HYPERV_NOT_RUNNING (0x80370102)` / `HCS_E_HYPERV_NOT_INSTALLED (0x80370101)` | « Le moteur de virtualisation Windows n'est pas démarré. Causes fréquentes : VirtualBox/VMware anciens, `bcdedit hypervisorlaunchtype` désactivé, exécution dans une machine virtuelle. » |
| Services `vmcompute`/`hns` arrêtés ou absents | SCM | Tentative de démarrage, sinon message + commande à donner à l'administrateur |
| Antivirus bloquant l'accès aux VHDX ou au binaire | `E_ACCESSDENIED` à la création, service tué | « Un logiciel de sécurité bloque Solon. Ajoutez `%ProgramData%\Solon` et `SolonService.exe` aux exclusions. » |
| RAM insuffisante | < 4 Go libres | Avertissement, réduction automatique de la mémoire allouée |
| Service Solon arrêté / désactivé | SCM | Démarrage à la demande depuis l'app (droit accordé à l'utilisateur interactif via ACL du service) |

### 7.3 Démarrage, arrêt, récupération

- **Démarrage** : le service démarre avec Windows (type Automatique différé), mais **ne lance pas la VM** tant que l'app ou le tray ne le demande pas (option « démarrer en arrière-plan à l'ouverture de session » désactivée par défaut). Séquence invité : noyau → initrd → agent PID 1 → `fsck.ext4 -p data.vhdx` → montage → `containerd` → `dockerd` → `GetHealth` OK. Objectif mesuré, pas promis ; ordre de grandeur attendu : 1 à 3 s jusqu'à l'API Docker.
- **Arrêt propre** : `PrepareShutdown{timeout=10 s}` → l'agent arrête les conteneurs (`docker stop` groupé), `dockerd`, `sync`, démonte, `poweroff` ; le service attend l'événement d'arrêt HCS, sinon `HcsTerminateComputeSystem` après 20 s. Le relais Docker refuse les nouvelles connexions dès le début de l'arrêt.
- **Arrêt brutal (coupure, crash hôte, `HcsTerminate`)** : le rootfs est en lecture seule, donc toujours cohérent. `data.vhdx` est en ext4 journalisé, monté `data=ordered`, avec `barrier` actif et `CachingMode` du disque HCS réglé pour ne pas mentir sur les `fsync` (à valider : `Uncached` vs `Cached`). Au redémarrage : `fsck -p`, rapport à l'hôte (`FsckReport`), puis `dockerd` reconstruit son état (`live-restore` désactivé : les conteneurs sont marqués arrêtés, ceux en `restart: always` repartent). Le service détecte au boot une VM orpheline (`Owner=Solon`) et la termine avant de recréer la sienne. Un fichier `state.json` avec horodatage et étape en cours permet de détecter « le dernier arrêt n'était pas propre » et de l'indiquer à l'utilisateur.
- **Crash du service seul** : au redémarrage il tente `HcsOpenComputeSystem` sur l'identifiant conservé ; si la VM est vivante, il se rattache (reconnexion HvSocket) sans redémarrer les conteneurs.
- **Mémoire** : allocation configurable (défaut : 25 % de la RAM, min 1 Go, max 8 Go), `AllowOvercommit` + hints HCS, `hv_balloon` + page reporting côté noyau, `vm.drop_caches` périodique côté agent si l'inactivité dépasse un seuil. Mesure : working set du processus `vmmem` correspondant + service + app.

---

## 8. Découpage Rust et commandes Tauri

### 8.1 Espace de travail Cargo

```
solon/
├── Cargo.toml                 (workspace)
├── crates/
│   ├── solon-core/            modèles, erreurs à codes stables, traits `VmBackend`, `EngineBackend`,
│   │                          `HostShareProvider`, `NetworkProvider`, protocole RPC (types partagés hôte/agent)
│   ├── solon-vm-hcs/          [windows] bindings HCS/HCN/virtdisk, document JSON, cycle de vie, orphelins
│   ├── solon-hvsock/          [windows] `HvStream`/`HvListener` Tokio, GUID de service ↔ port vsock
│   ├── solon-engine/          client `bollard`, flux logs/stats/events, exec hijacké, réécriture de chemins
│   ├── solon-prereq/          [windows] détection/activation des prérequis, mapping HRESULT → codes
│   ├── solon-service/         [windows] binaire service Windows : machine à états, relais pipe→hvsock,
│   │                          relais de ports, API de contrôle sur `\\.\pipe\solon-control`
│   ├── solon-agent/           [linux, musl statique] PID 1 invité : montages, dockerd, RPC, compose, arrêt
│   ├── solon-ipc/             protocole app ↔ service (JSON-RPC sur named pipe), client et serveur
│   └── solon-bench/           mesures : temps de démarrage, RAM, débit 9P (résultats copiés dans le README)
├── apps/desktop/              Tauri v2 (src-tauri/ + src/ React)
├── image/                     Dockerfiles noyau/rootfs/initrd, `solon.config`, `manifest`, scripts
├── installer/                 hooks NSIS, scripts d'activation de fonctionnalités, enregistrement HvSocket
├── docs/                      décisions (ADR), mesures, dépannage
└── tests/                     tests d'intégration (gated `SOLON_IT=1`, Windows + Hyper-V)
```

### 8.2 Traits pour la phase 2 (Linux natif)

```rust
pub trait VmBackend: Send + Sync {
    async fn status(&self) -> Result<VmStatus>;
    async fn ensure_running(&self, cfg: &VmConfig, progress: ProgressSink) -> Result<()>;
    async fn stop(&self, mode: StopMode) -> Result<()>;
    async fn open_engine_channel(&self) -> Result<Box<dyn AsyncStream>>; // vers l'API Docker
    async fn open_control_channel(&self) -> Result<Box<dyn AsyncStream>>;
    fn shares(&self) -> &dyn HostShareProvider;
    fn network(&self) -> &dyn NetworkProvider;
}
```

Sur Windows : `HcsVmBackend`. Sur Linux (phase 2) : `NativeBackend` où `ensure_running` lance ou trouve un `dockerd` local, `open_engine_channel` ouvre le socket Unix, `HostShareProvider` est l'identité (chemins inchangés) et `NetworkProvider` ne fait rien. Le service Windows disparaît sur Linux : l'app appelle le backend en processus. Tout ce qui est `#[cfg(windows)]` est confiné dans `solon-vm-hcs`, `solon-hvsock`, `solon-prereq`, `solon-service`.

### 8.3 Commandes Tauri (`apps/desktop/src-tauri`)

| Groupe | Commandes |
|---|---|
| Moteur | `engine_status`, `engine_start`, `engine_stop`, `engine_restart`, `engine_subscribe(channel)` (états + événements Docker), `prereq_report`, `prereq_fix(step)` |
| Conteneurs | `containers_list(all)`, `container_inspect(id)`, `container_start/stop/restart/kill(id)`, `container_remove(id, force, volumes)` (confirmation côté UI), `logs_open(id, opts, channel) -> stream_id`, `stream_close(stream_id)`, `stats_open(channel) -> stream_id`, `exec_open(id, cmd, tty, cols, rows, channel) -> exec_id`, `exec_input(exec_id, bytes)`, `exec_resize(exec_id, cols, rows)`, `exec_close(exec_id)` |
| Images | `images_list`, `image_inspect(id)`, `image_remove(id, force)`, `image_run(spec)` |
| Volumes / réseaux | `volumes_list/create/remove/inspect`, `networks_list/create/remove/inspect` |
| Compose | `compose_detect(dir)`, `compose_up(dir, channel) -> stream_id`, `compose_down(project, volumes)`, `compose_projects` (regroupement par label `com.docker.compose.project`) |
| Réglages | `settings_get/set` (CPU, RAM, langue, thème, démarrage auto), `open_in_explorer(path)`, `pick_folder` |
| Tray | géré en Rust : menu « Conteneurs actifs (n) », « Démarrer/Arrêter le moteur », « Ouvrir », « Quitter » |

Tous les flux (`logs`, `stats`, `exec`, `compose`, `engine_subscribe`) utilisent des **Channels Tauri v2** ; chaque flux a un identifiant et un `stream_close` explicite, appelé au démontage du composant React ; le backend ferme aussi les flux orphelins au changement de fenêtre.

### 8.4 Statistiques CPU/RAM

MVP : flux `bollard` `/containers/{id}/stats?stream=1` par conteneur en marche, agrégés par le backend en un snapshot par seconde envoyé sur un seul Channel. Si le coût devient visible (> 30 conteneurs), bascule vers `GetStatsSnapshot` de l'agent (lecture directe de cgroup v2), prévu dans le protocole dès le départ.

---

## 9. Frontend (React + TypeScript + Tailwind)

- Vite, React 19, TypeScript strict, Tailwind 4, `react-i18next` (**`en` par défaut, `fr`** ; décision du 2 septembre 2026, inverse du brief initial), `@tanstack/react-query` pour les listes (invalidation par événements Docker, jamais de polling), `xterm.js` + addon fit pour le terminal, `@tauri-apps/api` Channels.
- Direction Fluent/Windows 11 : police système (`Segoe UI Variable`), rayons 4–8 px, surfaces en couches, accent système via `--accent` lu à partir de Windows (Tauri), thème clair/sombre suivant `prefers-color-scheme`, contrastes WCAG AA vérifiés (outil dans la CI).
- Vues : Conteneurs (table dense, filtre instantané, groupes Compose repliables, actions inline), Détail conteneur (onglets Logs / Terminal / Inspecter / Stats), Images, Volumes, Réseaux, Réglages, écran de première installation (progression par étapes, messages d'erreur avec action).
- Accessibilité : navigation complète au clavier (`roving tabindex` dans les tables, raccourcis documentés), `aria-live` pour les changements d'état, confirmations destructives en dialogues modaux focalisés.
- Vocabulaire : « moteur », « conteneurs », « images », « volumes » ; jamais « VM », « WSL », « Hyper-V » dans l'UI (les journaux techniques, eux, sont explicites).

---

## 10. Tests

- **Unitaires** (toutes plateformes) : codec RPC, réécriture de chemins Windows→invité, document HCS généré (snapshot JSON), mapping HRESULT → codes d'erreur, machine à états (transitions), regroupement Compose.
- **Intégration VM** (`tests/vm_*.rs`, `#[ignore]` sauf `SOLON_IT=1`, exécution en Administrateur sur Windows avec Hyper-V) : création/démarrage/arrêt, rattachement après redémarrage du service, terminaison d'une orpheline, `Ping` HvSocket, `docker version` via le pipe, montage 9P aller-retour, **test de crash** (`HcsTerminateComputeSystem` pendant une écriture ext4, redémarrage, `fsck` propre, `dockerd` répond, volume intact), fuite de flux (ouverture/fermeture de 100 flux de logs).
- **Intégration moteur** : contre la VM réelle : cycle de vie conteneur, logs, exec avec resize, compose up/down sur un projet de test, événements.
- **Bancs de mesure** (`solon-bench`) : démarrage à froid → API prête, RAM au repos à 5 min (`vmmem` + service + app), 9P vs volume nommé. Résultats versionnés dans `docs/measurements.md`.
- **CI** : GitHub Actions ; build de l'image Linux et tests unitaires sur Linux ; tests d'intégration sur un runner Windows auto-hébergé avec Hyper-V (les runners hébergés n'exposent pas de virtualisation imbriquée de façon fiable).

---

## 11. Packaging et signature

- **NSIS** via le bundler Tauri (`installMode: perMachine`, donc élevé), hooks `installer/hooks.nsh` → `installer/setup.ps1` : vérification et activation des fonctionnalités (`Microsoft-Hyper-V`, `VirtualMachinePlatform`) avec drapeau « redémarrage requis » (code 3010 → `SetRebootFlag`) ; installation et démarrage de `SolonService` (Automatique). Aucun enregistrement HvSocket dans le registre : toutes les connexions sont ouvertes par l'hôte. L'image (`vmlinuz`, `initrd.img`, `rootfs.vhd`, `manifest.json`) et `solon-service.exe` sont installés **dans le dossier d'installation** ; le service cherche `image\` à côté de son exécutable avant `%ProgramData%\Solon\image`. Les données (`data.vhdx`, `state.json`, `settings.json`, journaux) restent dans `%ProgramData%\Solon`. Désinstallation : arrêt et suppression du service (qui arrête la machine et détache le réseau), **question explicite** avant de supprimer `%ProgramData%\Solon` (réponse par défaut : conserver, y compris en mode silencieux).
- Taille mesurée : **80 Mo** d'installeur pour 324 Mo décompressés (rootfs 307 Mo). Embarquée : un seul fichier, aucun téléchargement, cohérent avec « zéro requête sortante ».
- **Signature** : configuration séparée `apps/desktop/src-tauri/tauri.signed.conf.json` (`bundle.windows.signCommand` → `installer/sign.ps1`, `signtool` avec certificat OV/EV du magasin ou Azure Trusted Signing, horodatage RFC 3161) ; construire avec `npm run tauri build -- --config src-tauri/tauri.signed.conf.json`. Tauri signe alors `solon.exe`, l'installeur et le désinstalleur ; `solon-service.exe` étant une ressource, `sign.ps1` doit aussi lui être appliqué avant la construction (`installer/sign.ps1 -Path target\release\solon-service.exe`). Sans certificat, SmartScreen avertira ; la réputation ne se construit qu'avec un certificat stable.
- **Mises à jour** : hors MVP (impliquerait une requête réseau). Une nouvelle version se distribue par un nouvel installeur qui conserve `data.vhdx`.

---

## 12. Plan de travail par blocs (arrêt et validation après chacun)

| Bloc | Contenu | Critère de sortie |
|---|---|---|
| **0a — Spike VM** | Dépôt, workspace, `solon-vm-hcs` minimal : document JSON, création, démarrage d'un noyau (Alpine `linux-virt` en secours, WSL2 en cible) avec console série vers un pipe, arrêt, orphelins | Un noyau démarre et s'arrête depuis un test Rust ; besoins en privilèges confirmés ; champs HCS validés |
| **0b — Spike canaux** | `solon-hvsock`, agent « echo », Plan9 monté depuis l'invité, micro-bancs 9P, relais pipe→hvsock | `Ping` aller-retour ; fichier écrit depuis l'invité visible dans l'Explorateur ; **chiffres 9P publiés** ; go/no-go sur 9P et Tokio |
| **1 — Image** | Pipeline noyau + rootfs VHDX + initrd reproductible, agent PID 1 minimal, `dockerd` démarre, `docker version` via le pipe | `docker run hello-world` (image locale) depuis le CLI Windows via `\\.\pipe\solon` |
| **2 — Service** | Machine à états complète, prérequis, récupération après crash, réseau HNS, relais de ports, journaux, IPC app↔service, tests d'intégration | Tests d'intégration verts, y compris test de crash ; temps de démarrage et RAM **mesurés et annoncés** |
| **3 — App : provisionnement + conteneurs** | Tauri, écran de première installation, liste temps réel, actions, logs, exec xterm.js, i18n, thèmes | 3 des 7 fonctionnalités du MVP livrées |
| **4 — App : images, volumes, réseaux, compose, tray** | Vues restantes, compose via l'agent, regroupement, tray | 7/7 |
| **5 — Durcissement** | Catalogue d'erreurs, VPN/MTU, antivirus, entreprise, accessibilité, revue sécurité (ACL pipe, entrées RPC) | Chaque code d'erreur a un test et un texte |
| **6 — Livraison** | README, ARCHITECTURE (ce document mis à jour avec les mesures), CONTRIBUTING, LICENSE Apache-2.0, NSIS, signature | Installation propre sur une machine Windows Pro vierge |

État au 3 septembre 2026 : blocs 0a à 6 livrés et validés sur la machine de test (voir §1.3 à §1.10). Le critère « machine Windows Pro vierge » n'a pu être vérifié que partiellement : la machine de test avait déjà Hyper-V activé (le chemin « activation + redémarrage » de `setup.ps1` reste à éprouver ailleurs).

Git : dépôt déjà initialisé (`main`), commits atomiques par bloc et par crate, messages en français au format `type(portée): résumé` (ex. `feat(vm-hcs): création du compute system`).

---

## 13. Registre des risques

| # | Risque | Probabilité | Impact | Réponse |
|---|---|---|---|---|
| R1 | Performance 9P insuffisante pour les cas d'usage Node/PHP/Python volumineux | Élevée | Perception « plus lent que Docker Desktop WSL2 » sur les montages | Mesurer au bloc 0b ; volumes nommés mis en avant ; interface remplaçable ; chantier post-MVP (sync ou 9P/FUSE maison) |
| R2 | Windows Home exclu | Certaine pour le MVP | Part de marché grand public | Message clair ; phase 2 : serveur de fichiers maison sur HvSocket (VM HCS seule semble démarrer sur Home : à vérifier en VM imbriquée) ou OpenVMM/WHP |
| R3 | HvSocket incompatible avec Tokio/mio | **Levé** (bloc 0b) | — | `TcpStream::from_raw_socket` + `tokio::net::TcpStream::from_std` fonctionnent ; RTT ~450 µs |
| R4 | Champs HCS (`LinuxKernelDirect`, hints mémoire, `Plan9`) rejetés par certaines builds Windows | Faible à moyenne (validé sur Windows 11 Pro 26200 seulement) | Provisionnement qui échoue sur certaines versions | Matrice de versions testée ; document HCS adaptatif selon `SchemaVersion` supportée ; Windows 10 22H2 vérifié en VM imbriquée |
| R5 | Conflits réseau HNS (VPN, Docker Desktop, plages IP) | Moyenne | `docker pull` échoue | Plage dynamique, MTU, diagnostic ; pile utilisateur post-MVP |
| R6 | Budget RAM < 500 Mo non atteint | **Mesuré** : plancher 426 Mo au repos, 514 Mo juste après activité | Exigence tenue de justesse | Ballon et hints actifs sans gain sur le plancher ; pistes : 1 Go par défaut, compactage mémoire invité (§1.9, `docs/measurements.md`) |
| R7 | Antivirus/EDR qui bloque le service, les VHDX ou HvSocket | Moyenne en entreprise | Échec silencieux | Détection par HRESULT, messages d'exclusion, signature du binaire |
| R8 | Compilation du noyau trop longue en CI | Faible | Cadence de sécurité ralentie | Cache `ccache`, image de build épinglée, repli `linux-virt` |
| R9 | Certificat de signature indisponible | Dépend de toi | Alertes SmartScreen | Chaîne prête ; certificat à acquérir |
| R10 | Cohabitation avec Docker Desktop sur la même machine | Certaine sur la machine de test | Confusion de contexte `docker`, plages IP | Pipe distinct, contexte `solon` non défini par défaut, réseau HNS distinct |

---

## 14. Sources consultées

- Host Compute System API, vue d'ensemble et référence de schéma : https://learn.microsoft.com/en-us/virtualization/api/hcs/overview et https://learn.microsoft.com/en-us/virtualization/api/hcs/schemareference
- `hcsshim` (Microsoft, Go) : `internal/hcs/schema2` (`Plan9Share`, drapeaux), `internal/uvm/plan9.go`, `internal/guest/storage/plan9/plan9.go` : https://github.com/microsoft/hcsshim
- Bindings Rust : `windows` / `windows-sys`, module `Win32::System::HostComputeSystem` : https://docs.rs/windows-sys/latest/windows_sys/Win32/System/HostComputeSystem/
- Hyper-V sockets (`hv_sock`) dans le noyau Linux : https://cateee.net/lkddb/web-lkddb/HYPERV_VSOCKETS.html
- Noyau WSL2 : https://github.com/microsoft/WSL2-Linux-Kernel
- Alpine `linux-virt` sous Hyper-V, problème réseau : https://gitlab.alpinelinux.org/alpine/aports/-/issues/16096
- OpenVMM, guide et référence CLI (`--virtio-fs`, `--virtio-9p`, `--virtio-vsock-path`, `--nic`, WHP) : https://openvmm.dev/guide/ et http://openvmm.dev/guide/reference/openvmm/management/cli.html
- Claude Cowork sur Windows, configuration HCS observée et exigences : https://github.com/anthropics/claude-code/issues/40392 et https://www.betterclaw.io/blog/claude-cowork-not-working-windows
- Docker VMM (bêta 4.86, août 2026) : https://www.docker.com/blog/docker-vmm-public-beta/ et https://www.infoq.com/news/2026/08/docker-vmm-layer/
- Docker Desktop, historique du partage de fichiers : https://www.docker.com/blog/file-sharing-with-docker-desktop/
- podman machine Hyper-V (gvproxy, HvSocket, registre) : https://docs.podman.io/en/latest/markdown/podman-system-hyperv-prep.1.html
- Virtual Machine Platform vs Hyper-V vs WHP : https://www.virtualizationhowto.com/2024/01/virtual-machine-platform-vs-hyper-v-vs-windows-hypervisor-platform/
- `bollard`, transport personnalisé et named pipe : https://docs.rs/bollard/latest/bollard/struct.Docker.html
