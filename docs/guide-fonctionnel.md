# Solon — guide fonctionnel

*Version 0.1.10, 13 septembre 2026. Ce guide décrit ce que fait l'application, écran par écran, du point de vue de
la personne qui l'utilise. Les choix techniques sont dans `ARCHITECTURE.md`, les mesures dans
`docs/measurements.md`.*

## 1. Ce qu'est Solon, en une phrase

Solon fait tourner vos projets Docker sur Windows, sans Docker Desktop ni WSL : un installateur, un moteur
Docker complet dans une petite machine Linux que Solon crée et gère lui-même, et une interface qui part de **vos
projets**, pas d'une liste de conteneurs. Chaque projet reçoit une adresse `https://….solon.local` qui marche
toujours ; ce qui tournait avant un redémarrage retourne ; les fichiers Windows montés dans les conteneurs sont
rapides.

La promesse qui guide chaque choix : **tes projets tournent, tu ne t'occupes de rien.** Docker est un détail
d'implémentation ; il reste accessible (liste des conteneurs, images, volumes, réseaux, terminal) pour qui en a
besoin.

## 2. Vocabulaire

| Mot dans l'interface | Ce que c'est |
|---|---|
| **Moteur** | Le Docker de Solon : une machine Linux légère lancée par le service Windows `SolonService`. Il démarre en quelques secondes, s'arrête proprement, redémarre seul ce qui tournait. |
| **Projet** | Un dossier Windows contenant un `compose.yaml` (ou `docker-compose.yml`). C'est l'unité de travail de Solon : une carte sur l'accueil, une page avec ses services. |
| **Service** | Une ligne du fichier Compose (`web`, `db`…) ; en marche, c'est un conteneur. |
| **Adresse locale** | `https://<service>.<projet>.solon.local` pour un service Compose, `https://<nom>.solon.local` pour un conteneur isolé. Fournie par Solon, avec un certificat reconnu par le navigateur, port publié ou non. |
| **Endormi** | Un conteneur joint par Solon (adresse locale ou port publié) sans trafic depuis dix minutes est mis en pause ; la première requête le réveille en une fraction de seconde. Il compte comme en marche. |
| **Pile** | Un `compose.yaml` prêt à l'emploi de la galerie (WordPress, PostgreSQL, n8n, kits Django, Next.js…). |

## 3. Installation et premier démarrage

1. **Installateur** `Solon_x.y.z_x64-setup.exe` (GitHub Releases, bientôt winget). Windows 10 22H2 ou 11, éditions
   Pro, Entreprise ou Éducation ; virtualisation activée dans le BIOS. L'installateur active Hyper-V et la
   Plateforme de machine virtuelle si besoin et demande alors un redémarrage. Il n'est pas encore signé : sur
   l'écran SmartScreen, « Informations complémentaires » puis « Exécuter quand même ».
2. **Service Windows** `SolonService` installé et démarré ; il gère le moteur. Le moteur lui-même démarre au
   premier lancement de l'application (ou à l'ouverture de session si le réglage est coché).
3. **Premier écran** : l'accueil Projets vide propose quatre départs : ouvrir un dossier, choisir une pile,
   retrouver les projets déjà présents sur le PC, essayer hello-world.
4. **Mise à jour** : installer la nouvelle version par-dessus conserve images, conteneurs, volumes et réglages.
   **Désinstallation** : la question « supprimer aussi les données ? » **déplace** le dossier de données vers
   `%ProgramData%\Solon.removed-<date>` (rien n'est effacé) ; répondre Non le laisse en place pour une
   installation future.

## 4. L'accueil : Projets

C'est la première entrée du menu (Ctrl+1) et la page d'ouverture.

**Une carte par projet** :

- nom, pastille d'état, « 2/2 en marche · Up 14 minutes » ou « créé le … » ;
- **l'adresse principale** en gros, avec le cadenas HTTPS et un bouton Copier : celle du service qui ressemble
  le plus à une application web (ports 80, 8080, 3000, 8000, 8069… ; les bases de données ne sont jamais
  choisies). Un clic l'ouvre dans le navigateur ;
- la liste des services avec leur état et leur adresse propre ;
- **Ouvrir** (navigateur), **Up** ou **Arrêter**, **Détails** (la page du projet).

**Cartes « Pas encore démarré »** : les dossiers ouverts auparavant mais sans conteneur ; Up les lance, Oublier
les retire de la liste. Un dossier qui n'existe plus disparaît de lui-même.

**« Autres conteneurs »** : ce qui a été lancé hors projet (`docker run`, bouton Run d'une image), avec son
adresse et Start / Stop.

**Barre du haut** :

- **Ouvrir un projet…** : choisir un dossier. S'il contient un fichier Compose, il s'ouvre ; sinon Solon regarde
  ce qu'il contient (package.json, requirements.txt, Dockerfile, composer.json, go.mod, Cargo.toml, pom.xml,
  .csproj, Gemfile…) et propose un environnement, modifiable avant création. Rien n'est jamais écrasé.
- **Nouvelle pile…** : la galerie (voir § 6).
- **Chercher les projets sur ce PC…** : Solon lit la table des fichiers des disques internes (quelques
  secondes) et liste chaque dossier qui contient un `compose.yaml`, un `Dockerfile` ou un
  `devcontainer.json`, hors dossiers système, de dépendances et de cache. On coche ceux à garder ; un dossier
  sans Compose a un bouton « Configurer… » qui l'envoie dans la galerie. Rien ne quitte le PC.
- **Restaurer une sauvegarde…** : voir § 5.4.

## 5. La page d'un projet

En tête : le nom, l'état, la **branche Git** si le dossier est un dépôt (§ 5.5), l'adresse principale, et les
boutons **Up**, **Rebuild** (reconstruit les images puis Up), **Down**, **Explorateur**, **VS Code**,
**Sauvegarder…**.

### 5.1 Services

Le tableau des services : conteneur, image, état, adresse locale et ports publiés (chacun est un lien), et par
ligne Stop / Restart / Start / journaux. Un clic sur un service ouvre sa fiche (§ 7) ; Retour revient au projet.

### 5.2 Up qui échoue sur un port pris

Avant chaque Up, Solon vérifie les ports hôte du fichier Compose (`"8080:80"`). Si un port est déjà pris sur le
PC par un autre programme ou une autre pile, **rien n'est démarré** et un bandeau propose « Utiliser 8081 au
lieu de 8080 » (le fichier est modifié pour vous, puis Up) ou « Up quand même ». Les ports que le projet publie
déjà lui-même ne comptent pas comme conflit.

### 5.3 Les trois onglets

- **Journaux** : les journaux de tous les services mêlés, chaque ligne préfixée du service et colorée. Filtres
  **Tout / Avertissements / Erreurs** avec compteurs, une pastille par service pour l'afficher ou le masquer,
  recherche, suivi automatique (un défilement vers le haut le suspend).
- **Compose** : le fichier `compose.yaml` modifiable sur place. Enregistrer (Ctrl+S), **Save and Up**, Recharger.
- **Environnement** : les variables du `.env` et des blocs `environment:` de chaque service, et les ports hôte
  publiés, **modifiables sans toucher au YAML**. Les valeurs qui ressemblent à des secrets sont masquées (œil
  pour les voir). Ajouter, retirer, Save and Up. Les fichiers sont réécrits ligne à ligne : commentaires et
  mise en forme conservés.

### 5.4 Sauvegarde et restauration

**Sauvegarder…** écrit **un seul fichier zip** : les fichiers Compose, le `.env` et les données de chaque volume
du projet (bases de données comprises), prises directement dans le moteur, projet en marche ou non.
**Restaurer une sauvegarde…** (accueil) demande le zip puis un dossier vide, recrée les volumes et les fichiers,
ouvre le projet ; il ne reste qu'à faire Up. Déplacer un projet vers un autre PC est une copie de fichier. Si le
dossier a un autre nom, les volumes sont renommés en conséquence.

### 5.5 Un environnement par branche Git

Si le dossier est un dépôt Git, une pastille montre la branche courante avec la case **« Un environnement par
branche »**. Cochée :

- le projet Compose devient `<projet>-<branche>` : chaque branche a ses conteneurs, ses volumes et ses adresses
  (`web.blog-feature-login.solon.local`) ;
- changer de branche dans un terminal fait apparaître, en quelques secondes, « Branche changée : main →
  feature/login. L'environnement de main tourne encore », avec **Basculer** (arrêter l'ancien, démarrer le
  nouveau), **Basculer avec une copie des données de main** (les volumes sont copiés dans le moteur, jamais
  écrasés), ou Ignorer ;
- les autres branches du dossier sont listées avec leur état et un bouton Arrêter ; l'accueil affiche un badge
  de branche sur chaque carte.

## 6. La galerie de piles

« Nouvelle pile… » ouvre une galerie avec recherche : **Blank** (un `compose.yaml` vide à remplir), des **kits de
démarrage** qui génèrent un projet au premier Up (Django + PostgreSQL, Flask + Redis, FastAPI + PostgreSQL,
Next.js), des **applications prêtes** (WordPress, Odoo, PostgreSQL + Adminer, MariaDB + Adminer, MongoDB + Mongo
Express, Redis, n8n, Nextcloud, Ghost, Gitea, Uptime Kuma, Jupyter Lab, site statique Nginx) et des **outils**
(Mailpit). On choisit, on désigne le dossier parent et le nom, on relit le `compose.yaml` généré (modifiable),
**Créer** ou **Créer et démarrer**. Un port déjà pris est signalé pendant la frappe avec le suivant libre en un
clic. Le projet s'ouvre et démarre ; l'adresse apparaît dès que le service répond.

## 7. La fiche d'un conteneur

Ouverte depuis un projet ou depuis la liste des conteneurs.

- **En tête** : nom (cliquer pour renommer), image, état ou « Endormi », lien vers le projet, **Garder éveillé**
  (exclut ce conteneur de la mise en veille), Stop / Restart / Supprimer.
- **Vue d'ensemble** : identifiant, image, dates, santé, commande, utilisateur, politique de redémarrage ;
  réseau (adresse, passerelle, alias, ports) ; montages ; variables d'environnement (Copier) ; étiquettes ;
  copie de fichiers avec Windows.
- **Journaux** : suivi, horodatage, retour à la ligne, filtres Tout / Avertissements / Erreurs, recherche.
- **Fichiers** : parcourir le système de fichiers du conteneur, créer un dossier, envoyer des fichiers ou un
  dossier depuis Windows (ou les déposer), télécharger, copier un dossier vers Windows, supprimer.
- **Terminal** : un shell dans le conteneur.
- **Shell de débogage** : un shell outillé (`ps`, `curl`, `tcpdump`, `strace`, `jq`, `vim`…) qui partage les
  processus, le réseau et les volumes du conteneur, **même si l'image n'a aucun shell**. L'image outil se
  construit une fois, au premier usage, avec accès à internet.
- **Inspecter** : le JSON complet de `docker inspect`, avec Copier.

## 8. Conteneurs, Images, Volumes, Réseaux

Les vues Docker classiques, pour qui en a besoin.

- **Conteneurs** : liste en temps réel groupée par projet (en-tête cliquable vers le projet), CPU et mémoire
  avec courbe, état ou Endormi, adresses et ports (liens), Stop / Restart / Start / journaux / Supprimer,
  filtre, « Afficher les arrêtés ». Un conteneur démarré qui se termine aussitôt (hello-world, commande
  ponctuelle) est signalé avec son code de sortie et ses journaux ouverts.
- **Images** : nom court et complet, **Utilisée / Arrêtée / Inutilisée** (par quels conteneurs), taille, date ;
  Run (nom, ports, variables, commande), Inspecter, Supprimer ; filtre « Inutilisées seulement ».
- **Volumes** : utilisation, pilote, date, point de montage ; **Fichiers** (parcourir et échanger avec Windows
  sans conteneur en marche), Inspecter, Supprimer ; création.
- **Réseaux** : utilisation, pilote, sous-réseau ; Inspecter, Supprimer ; création.

Toutes les suppressions demandent confirmation ; supprimer un conteneur propose d'emporter ses volumes.

## 9. Activité

Le tableau de bord du moteur : CPU (avec charge), mémoire, stockage, réseau, puis les **courbes** CPU, mémoire et
réseau par conteneur sur 1 min, 10 min ou 1 h. Survol pour les valeurs exactes, clic sur un nom pour l'isoler,
clic sur sa couleur pour le masquer ; la courbe « Moteur » donne le total. En bas, chaque conteneur en marche
avec CPU, mémoire, débits réseau et sa courbe de la dernière minute.

## 10. Terminal

Un shell **dans la machine du moteur** (`docker`, `ps`, `df`, `dmesg`…), pour les curieux et le dépannage. Ctrl+`
l'ouvre et le referme depuis n'importe quelle vue ; la session reste vivante en arrière-plan.

## 11. Réglages

- **Langue** (anglais par défaut, français), **apparence** (clair, sombre, suivre Windows), **couleur d'accent**
  (rose Solon ou couleur de Windows).
- **Diagnostic** : exporte un zip (journaux, état, réglages, prérequis, `docker info`, versions ; aucun
  identifiant, aucune donnée de conteneur) à joindre à un rapport de bug.
- **Mises à jour** : vérification au démarrage (une requête vers github.com, rien envoyé sur vous ; désactivable),
  « Vérifier maintenant », bouton Télécharger quand une version existe. Rien ne s'installe sans vous.
- **Disque** : occupation du stockage du moteur, place libre du lecteur Windows, taille réelle du fichier
  disque ; **Récupérer l'espace** supprime les images inutilisées et le cache de construction puis rend la place
  à Windows (conteneurs et volumes jamais touchés). Un bandeau prévient quand le lecteur est presque plein et
  propose le nettoyage sur place.
- **Ressources du moteur** : mémoire, processeurs (tous les cœurs moins deux par défaut), limite de stockage,
  démarrage à l'ouverture de session ; **mise en veille** des conteneurs inactifs et délai ; **relance de ce
  qui tournait** à l'arrêt ; partage de fichiers de repli (9P).

## 12. Ce qui se passe tout seul

- **Relance** : quand le moteur s'arrête (redémarrage de Windows, mise à jour de Solon, Stop depuis
  l'application), Solon note les projets et conteneurs en marche et les relance une fois le moteur revenu, dans
  l'ordre des dépendances. Docker seul ne le fait que pour les conteneurs marqués `restart: always`.
- **Adresses locales** : posées et retirées avec les conteneurs, HTTPS avec l'autorité locale de Solon,
  en-têtes `X-Forwarded-*` transmis (WordPress et compagnie génèrent des liens `https://`).
- **Mise en veille** des conteneurs inactifs et réveil à la première requête.
- **Notifications Windows**, rares par choix : un conteneur qui tombe avec une erreur (hors vos propres
  actions et hors arrêt du moteur), le moteur en échec ou en redémarrage, le disque presque plein, une relance
  terminée.
- **Barre des tâches** : état du moteur, un sous-menu par projet avec Démarrer / Redémarrer / Arrêter, les
  conteneurs isolés, démarrer ou arrêter le moteur. Fermer la fenêtre garde Solon dans la barre.

## 13. Raccourcis

| Raccourci | Effet |
|---|---|
| Ctrl+K | Recherche : sections, actions du moteur, conteneurs, images, volumes, réseaux, projets |
| Ctrl+Alt+S | Depuis n'importe quelle application : Solon au premier plan avec la recherche ouverte |
| Ctrl+1 … Ctrl+8 | Projets, Conteneurs, Volumes, Images, Réseaux, Activité, Terminal, Réglages |
| Ctrl+` | Terminal de la machine |
| Ctrl+B | Replier ou déplier le menu |
| Ctrl+S | Enregistrer (onglet Compose) |
| Échap | Fermer une boîte de dialogue |

## 14. Le `docker` de Windows

L'installateur ajoute `docker` et `docker compose` au PATH : tout ce que vous lancez en ligne de commande
apparaît dans Solon, et inversement. Les chemins Windows (`C:\…`) dans `-v`, `--mount` et les fichiers Compose
sont traduits et le lecteur partagé à la volée. Les dossiers Windows montés passent par le partage de fichiers
de Solon, bien plus rapide que le 9P de WSL ; pour les bases de données, préférez tout de même des volumes.

## 14 bis. VS Code Dev Containers

Un dossier avec `.devcontainer/devcontainer.json` s'ouvre dans VS Code par « Rouvrir dans un conteneur » :
l'extension se sert du `docker` de Solon sans rien régler. Le dossier est monté dans le conteneur dans les deux
sens, les ports déclarés sont transmis, le `postCreateCommand` s'exécute, et le conteneur apparaît dans Solon
comme les autres, avec son adresse locale. « Chercher les projets sur ce PC » repère aussi ces dossiers.

La vue **Conteneurs** de VS Code (extension Containers) suit l'état des conteneurs en direct. Le pipe Docker
de Solon existe dès le lancement du service, et une commande `docker` lancée pendant que le moteur démarre
l'attend au lieu d'échouer : une fenêtre VS Code ouverte à l'ouverture de session garde le direct. Si Solon
est mis à jour pendant que VS Code est ouvert, cette fenêtre retombe sur un rafraîchissement par minute ;
*Developer: Reload Window* lui rend le direct. Quand Solon est arrêté, `docker` répond « Solon is stopped.
Open the Solon app to start it. »

## 15. Limites connues

- Windows Famille non pris en charge ; ports UDP publiés non relayés vers `localhost` ; un seul moteur par PC.
- Installateur non signé (SmartScreen) ; signature via la SignPath Foundation demandée.
- Pas de Kubernetes, pas de machines Linux séparées, pas d'images ARM.
- Un conteneur endormi n'exécute pas ses tâches planifiées internes jusqu'à ce qu'on l'appelle (« Garder
  éveillé » si cela compte).
- `curl.exe` de Windows demande `--ssl-no-revoke` pour les certificats locaux ; les navigateurs et .NET les
  acceptent.
- Le partage d'un projet à un collègue sur le réseau, l'import depuis Docker Desktop et la mise à jour des
  images ne sont pas encore là.

## 16. Où chercher de l'aide

- Page Dépannage du site : <https://v94lere.github.io/solon/fr/depannage/> (SmartScreen, moteur qui ne démarre
  pas, port pris, conteneur qui se termine aussitôt, réseau et adresses, disque plein, Docker Desktop ou WSL
  présents, codes d'erreur).
- Rapport de bug : Réglages → Diagnostic → Exporter un diagnostic…, puis le formulaire GitHub avec le zip.
