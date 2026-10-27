# Radiographie

Une carte en temps réel de ce que les services d'un projet se disent entre eux : qui appelle qui,
ce qui est demandé, ce qui est répondu, avec une barre de lecture que l'on met en pause, ralentit
et rembobine. L'axe est le **débogage des échanges**, pas la supervision : on ne cherche pas à
savoir si tout va bien, on cherche pourquoi un appel a échoué.

Tout reste sur la machine. Rien n'est envoyé nulle part, rien n'est écrit en dehors de
`%ProgramData%\Solon`.

Ce document suit les étapes. Une section par phase, écrite au moment où elle est livrée.

## Phase 1 — qui appelle qui (0.1.16)

Les connexions TCP ouvertes depuis la machine, avec leur issue : établie, refusée, fermée. Une
carte des services et un journal des appels. L'événement « lancement de processus » de la phase 0
disparaît : il avait servi à vérifier la chaîne, il n'apprenait rien sur les échanges et coûtait du
bruit.

### Une seule sonde suffit

`sock/inet_sock_set_state`, la trace que le noyau émet à chaque changement d'état d'une socket
TCP. Elle donne les deux adresses, les deux ports, l'ancien et le nouvel état. Lire ses champs par
décalage — le noyau publie leur disposition dans
`/sys/kernel/tracing/events/sock/inet_sock_set_state/format`, c'est une interface stable — évite
d'avoir à connaître les structures internes du noyau, donc évite `bpftool`, `vmlinux.h` et toute
chaîne C.

Trois transitions sont retenues :

| De | Vers | Ce que ça veut dire |
|---|---|---|
| *(peu importe)* | `SYN_SENT` | un programme vient d'appeler : on met de côté qui il est |
| `SYN_SENT` | `ESTABLISHED` | l'appel a abouti |
| `SYN_SENT` | `CLOSE` | **l'appel a échoué** : refusé, filtré, ou abandonné sans réponse |
| *(peu importe)* | `CLOSE` | la connexion se termine, avec sa durée |

La troisième ligne est celle qui justifie le travail. Un appel refusé ne laisse aucune trace dans
les journaux quand la bibliothèque appelante avale l'erreur — c'est exactement le cas que l'on
cherche, et il apparaît ici en rouge sans rien avoir à instrumenter.

### Pourquoi l'appelant est mis de côté

Seule la transition vers `SYN_SENT` se produit dans le contexte du processus appelant. Les
suivantes arrivent en interruption logicielle, à la réception d'un paquet, où le processus courant
n'a plus rien à voir avec la connexion. Le cgroup et le nom du programme ne sont donc lisibles
qu'à l'ouverture : ils sont rangés dans une table indexée par l'adresse de la socket, et repris à
l'issue. La table est `LruHashMap` et non `HashMap` : une connexion dont on ne voit jamais l'issue
laisserait sinon une entrée pour toujours.

### Mettre un nom sur les deux bouts

Deux sources, parce qu'aucune ne suffit seule. Le numéro de cgroup désigne l'appelant avec
certitude, même quand son conteneur partage la pile réseau de la machine et n'a donc pas d'adresse
à lui. La liste des conteneurs en marche avec leur adresse, que l'agent tient déjà pour les
domaines locaux, est la seule façon de nommer l'appelé. Quand les deux se contredisent — une
adresse reprise par un conteneur qui vient de remplacer un autre — c'est le cgroup qui l'emporte.

Les connexions **entrantes** ne sont pas rapportées. Entre deux conteneurs, elles sont l'autre
extrémité d'une sortante déjà vue : les compter ferait double. Ce qui vient de l'extérieur de la
machine — un navigateur — n'est pas vu du tout ici, et c'est le mandataire de Solon qui le
rapportera, avec bien plus de détail.

### Ce que ça coûte

Mesuré sur la machine de développement, 20 000 tentatives de connexion vers un port fermé de la
boucle locale — le cas le plus défavorable, puisque c'est l'opération réseau la moins chère qui
soit, et que chacune produit un fait :

| | par connexion |
|---|---|
| sans sonde | 4,8 à 6,5 µs |
| avec sonde | 9,7 µs |

Soit **environ 4 µs ajoutées par connexion**. Le rapport impressionne, l'absolu beaucoup moins :
une requête HTTP réelle coûte des centaines de microsecondes à plusieurs millisecondes, et un
projet très actif qui ouvrirait mille connexions par seconde dépenserait 4 ms par seconde, soit
0,4 % d'un cœur. Le lecteur dans la machine est resté sous 4 Mo. Le budget de la note de mission —
moins de 3 % de processeur, moins de 50 Mo — est tenu avec de la marge.

### Vérifié

Sur le noyau d'une machine Solon en marche, dans un conteneur privilégié, sans toucher à
l'installation : un appel vers l'extérieur ressort en « établie » puis « fermée » avec sa durée
(63 ms), un appel vers un port fermé ressort en « échouée », et le cgroup de l'appelant est celui
du conteneur qui a lancé la commande.

Un défaut réel attrapé au passage : l'objet BPF doit être embarqué avec `include_bytes_aligned` et
non `include_bytes`. Sa lecture se fait sans copie et exige un tampon aligné ; avec un alignement
d'un octet, le chargement passe ou échoue selon la taille de l'objet. La phase 0 fonctionnait par
chance, la phase 1 a échoué d'un coup — une panne qui serait apparue à la première sonde ajoutée,
loin de sa cause.

### Ce qui n'est pas encore là

Le contenu des échanges. Les noms de domaine : une adresse extérieure s'affiche en chiffres tant
que les requêtes DNS ne sont pas observées. Et la barre de lecture — pour l'instant, une pause et
un filtre par projet.

## Phase 0 — la plomberie

L'objectif de cette phase n'était pas de montrer des échanges, mais de prouver que la chaîne
complète tient : un programme dans le noyau de la machine → l'agent → le service Windows →
l'écran. Un seul type de fait est observé, le lancement d'un processus. C'est volontairement le
plus bête possible : il se déclenche souvent, il se vérifie à l'œil, et il n'engage aucun choix
sur la suite.

### Ce qui a décidé de la forme

**Le noyau était déjà prêt.** `6.18.40.1-solon` est compilé avec `BPF`, `BPF_SYSCALL`, `BPF_JIT`,
`BPF_EVENTS`, `CGROUP_BPF`, `KPROBES`, `UPROBES`, `PERF_EVENTS`, et surtout `DEBUG_INFO_BTF` avec
`/sys/kernel/btf/vmlinux` présent. Aucune recompilation de noyau n'a été nécessaire, et le CO-RE
est possible pour les phases suivantes. C'était le risque le plus lourd du projet ; il n'existait
pas.

**bpf-linker ne se construit pas sous Windows.** Son script de construction appelle
`std::os::unix::ffi::OsStrExt` ; la compilation échoue avant même de chercher LLVM. C'est la seule
pièce de Solon qui exige un hôte Linux. Elle est donc fabriquée **dans un conteneur Solon**, comme
l'image Linux l'est déjà : `crates/solon-trace-ebpf/build.ps1` construit l'atelier décrit par le
`Dockerfile` voisin et produit l'objet. Aucune chaîne C n'apparaît sur la machine de
développement, et la propriété « tout se compile depuis Windows » n'est entamée que pour 1,9 Ko
d'objet BPF.

**Le couple Rust / LLVM ne se choisit pas librement.** bpf-linker relit le bitcode produit par
rustc : sa version de LLVM doit être au moins aussi récente. Or apt.llvm.org ne publie pas de
`llvm-23` (vérifié sur bookworm, trixie, noble et plucky : 20, 22 et 24 seulement), et bpf-linker
0.12 ne connaît pas encore LLVM 24. La chaîne est donc figée sur `nightly-2026-07-01`, qui utilise
LLVM 22.1.8. La relever demandera d'attendre que bpf-linker gère LLVM 24.

**L'objet BPF est versionné dans le dépôt.** La compilation croisée de l'agent depuis Windows ne
peut pas le produire, et l'agent l'embarque par `include_bytes!`. Un binaire commité dans un dépôt
ouvert se vérifie : le travail `ebpf` de la CI le reconstruit à chaque fois et refuse la
modification s'il diffère de ses sources.

**Le stockage vit sur l'hôte**, dans le service, pas dans la machine. L'invité reste sans état, le
disque de données n'est pas pollué par de l'observation, et l'interface parle déjà au service.
Pour l'instant un simple anneau de 20 000 faits en mémoire ; une base sur disque n'aura de sens
qu'avec du volume à garder entre deux démarrages, c'est-à-dire avec les sondes réseau.

**Un port vsock dédié, 5007.** Le flux des faits observés est bien plus bavard que les événements
du moteur : mêlés sur le même canal, une rafale d'échanges retarderait un changement d'état, et un
abonné en retard ferait perdre les deux. Partout où le choix se pose — tampon du noyau plein,
abonné lent — ce sont les faits observés qui sont abandonnés. Une sonde ne doit jamais ralentir ce
qu'elle regarde.

### Le chemin d'un fait

1. `crates/solon-trace-ebpf` — le programme chargé dans le noyau. Attaché au tracepoint
   `sched/sched_process_exec`, il écrit un `RawEvent` de 40 octets dans un tampon circulaire de
   256 Kio.
2. `crates/solon-trace-common` — la disposition de ce `RawEvent`, partagée entre le programme du
   noyau et l'agent, qui sont compilés séparément. Un test fige sa taille : si elle change d'un
   côté sans l'autre, l'agent lit n'importe quoi.
3. `crates/solon-agent/src/trace.rs` — charge l'objet avec aya (Rust pur, pas de libbpf, donc pas
   de chaîne C dans la compilation croisée), monte `tracefs` à la demande, lit le tampon, traduit
   l'horloge du noyau en temps réel et le numéro de cgroup en identifiant de conteneur, puis sert
   le tout sur vsock 5007.
4. `crates/solon-service/src/trace.rs` — garde l'historique et le diffuse aux abonnés.
5. `apps/desktop/src/views/RadiographyView.tsx` — l'écran, avec pause et effacement.

La correspondance cgroup → conteneur passe par le numéro d'inode : `bpf_get_current_cgroup_id`
renvoie l'inode du dossier de cgroup, et dockerd place chaque conteneur dans
`/sys/fs/cgroup/docker/<id>`. L'agent parcourt l'arbre et relève les inodes, en relisant au plus
toutes les deux secondes quand un identifiant inconnu apparaît.

### Vérifié

Le code de chargement a été exécuté dans un conteneur privilégié, sur le noyau d'une machine Solon
en marche, sans toucher à l'installation : le vérificateur accepte le programme, `tracefs` se
monte, les faits remontent, et les numéros de cgroup correspondent bien aux dossiers des
conteneurs (`94` → `041feeec…`, `238` → `f635cab5…`).

### Connu d'avance pour la suite

- **Le HTTPS sortant.** Les uprobes sur `SSL_read` / `SSL_write` couvrent ce qui utilise OpenSSL —
  Python, Node, PHP, Ruby, nginx — mais **ni Go ni Java**, qui chiffrent sans. La limite sera
  documentée plutôt que contournée.
- **Le HTTPS entrant est déjà résolu**, et gratuitement : le mandataire de Solon termine déjà le
  TLS pour `*.solon.local` avec son autorité locale. Tout le trafic navigateur → conteneur est
  donc lisible en clair là où il passe, sans sonde. C'est la source la plus riche du lot et elle
  ne coûte rien.
