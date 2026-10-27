# Radiographie

Une carte en temps réel de ce que les services d'un projet se disent entre eux : qui appelle qui,
ce qui est demandé, ce qui est répondu, avec une barre de lecture que l'on met en pause, ralentit
et rembobine. L'axe est le **débogage des échanges**, pas la supervision : on ne cherche pas à
savoir si tout va bien, on cherche pourquoi un appel a échoué.

Tout reste sur la machine. Rien n'est envoyé nulle part, rien n'est écrit en dehors de
`%ProgramData%\Solon`.

Ce document suit les étapes. Une section par phase, écrite au moment où elle est livrée.

## Phase 0 — la plomberie (0.1.16)

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

### Ce qui n'est pas encore là

Les sondes réseau, le contenu des échanges, la carte, la barre de lecture. Et deux choses à
décider avant d'y arriver :

- **Le HTTPS sortant.** Les uprobes sur `SSL_read` / `SSL_write` couvrent ce qui utilise OpenSSL —
  Python, Node, PHP, Ruby, nginx — mais **ni Go ni Java**, qui chiffrent sans. La limite sera
  documentée plutôt que contournée.
- **Le HTTPS entrant est déjà résolu**, et gratuitement : le mandataire de Solon termine déjà le
  TLS pour `*.solon.local` avec son autorité locale. Tout le trafic navigateur → conteneur est
  donc lisible en clair là où il passe, sans sonde. C'est la source la plus riche du lot et elle
  ne coûte rien.
