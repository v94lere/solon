# Solon pour le Microsoft Store (paquet MSIX)

Ce dossier produit `Solon_<version>_x64.msix`, le même Solon que l'installeur NSIS, empaqueté au
format du Microsoft Store.

## Pourquoi

**Le Store signe le paquet lui-même.** C'est la raison principale : on téléverse un paquet non signé,
Microsoft le signe avec son propre certificat, et l'avertissement SmartScreen disparaît pour tout le
monde. C'est exactement ce que la fondation SignPath a refusé de financer faute de notoriété.

Le reste vient avec : installation et désinstallation propres (rien ne traîne), mises à jour
distribuées par le Store, et une vitrine où l'on peut être trouvé par quelqu'un qui cherche
« docker » sur Windows.

## Ce que ça change dans Solon

Un paquet MSIX **s'installe sans élévation**, ce qui a trois conséquences.

| | Installeur NSIS | Paquet MSIX |
|---|---|---|
| Service `SolonService` | installé par `setup.ps1`, élevé | déclaré dans le manifeste ; Windows le crée, le démarre et le retire avec le paquet |
| Composants Windows (Plateforme de machine virtuelle, Hyper-V) | activés à l'installation | **impossible à l'installation** : l'application propose un bouton qui lance `installer\setup-features.ps1` élevé, Windows demande l'autorisation |
| `docker` dans le terminal | ajouté au PATH machine | alias d'exécution déclaré dans le manifeste, retiré tout seul à la désinstallation. Un manifeste n'accepte qu'un alias par application, et une application sans entrée au menu Démarrer est refusée par le Store : il n'y a donc pas d'alias `solon`, seulement `docker`. |
| Mises à jour | téléchargées et vérifiées par l'application | le Store s'en charge ; l'application détecte qu'elle est empaquetée (`GetCurrentPackageFullName`) et masque ses propres boutons |

Le reste est identique : le moteur, le disque de données dans `%ProgramData%\Solon`, les adresses
`*.solon.local`, l'autorité de certification locale. Le service tourne en compte système dans les
deux cas, parce que l'API de virtualisation de Windows (HCS) refuse la création d'une machine sans.

## Construire

```powershell
cargo build --release -p solon -p solon-service -p solon-docker-shim
cd apps\desktop; npm run tauri build; cd ..\..
.\packaging\msix\build-msix.ps1            # paquet de test, signé avec un certificat local
.\packaging\msix\build-msix.ps1 -Store     # paquet pour le Partner Center, non signé
```

Le script prend les binaires déjà compilés, pose le manifeste, génère l'index des ressources
(`resources.pri`, exigé par la certification), appelle `makeappx`, et pour un paquet de test crée au
besoin un certificat auto-signé. Sortie : `target\release\bundle\msix\`.

Le SDK Windows fournit `makeappx`, `makepri` et `signtool` ; il vient avec Visual Studio Build Tools.

## Essayer le paquet sur un PC

> **Attention** : le paquet installe un service nommé `SolonService`, comme l'installeur NSIS. Les
> deux ne peuvent pas cohabiter. **Désinstallez Solon** (Paramètres → Applications) avant d'essayer
> le paquet, ou faites l'essai dans une machine virtuelle. Le disque de données de `%ProgramData%`
> est conservé par la désinstallation : vos images et volumes vous attendent de l'autre côté.

```powershell
# 1. faire confiance au certificat de test (PowerShell administrateur)
Import-Certificate -FilePath .\target\release\bundle\msix\Solon_0.1.13.0_x64.cer -CertStoreLocation Cert:\LocalMachine\Root
# 2. installer
Add-AppxPackage .\target\release\bundle\msix\Solon_0.1.13.0_x64.msix
# 3. retirer
Get-AppxPackage *Solon* | Remove-AppxPackage
```

À vérifier après installation : le service `SolonService` existe et démarre, `docker version` répond
depuis un terminal neuf, le moteur démarre, et sur une machine sans virtualisation activée le bouton
« Activer les composants Windows requis » apparaît dans l'écran de vérification du système.

## Soumettre au Store

1. **Compte Partner Center.** Inscription au programme développeur Windows, avec des frais uniques
   (de l'ordre de vingt euros pour un compte individuel, à vérifier au moment de l'inscription) et
   une vérification d'identité qui prend quelques jours.
2. **Réserver le nom** « Solon » dans Partner Center. S'il est pris, choisir une variante ; le nom
   réservé devient le `Name` de l'identité du paquet.
3. **Relever l'identité** dans Partner Center → Identité du produit : `Package/Identity/Name`,
   `Package/Identity/Publisher` (de la forme `CN=<GUID>`) et `PublisherDisplayName`.
4. **Construire avec ces valeurs** :
   ```powershell
   .\packaging\msix\build-msix.ps1 -Store `
     -IdentityName "12345ValereNeveux.Solon" `
     -Publisher "CN=XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX" `
     -PublisherDisplayName "Valère Neveux"
   ```
5. **Demander les capacités restreintes.** Le manifeste en déclare trois, chacune soumise à l'accord
   de Microsoft, à demander dans la soumission (champ « Notes pour la certification ») :
   - `packagedServices` — déclarer un service Windows dans le paquet ;
   - `localSystemServices` — le faire tourner en compte système ;
   - `allowElevation` — demander l'élévation pour activer les composants Windows.

   Justification à donner, en substance : Solon crée une machine virtuelle par l'API HCS de Windows,
   qui exige le compte système ; c'est la même mécanique que WSL 2, dont le service `wslservice`
   tourne aussi en système. Sans service, pas de moteur.
6. **Téléverser** le `.msix` dans Soumissions → Paquets, remplir la fiche (description, captures,
   classification d'âge, déclarations de confidentialité : aucune collecte, aucune télémétrie), et
   soumettre.
7. **Certification** : quelques heures à quelques jours. Un refus est motivé et corrigeable.

Avant de soumettre, passer le **Windows App Certification Kit** (`appcert.exe`, fourni avec le SDK)
sur le paquet installé : il signale à l'avance la plupart des motifs de refus.

## Refus rencontrés au téléversement, et leur correction

| Message de Partner Center | Cause | Correction |
|---|---|---|
| « application sans périphérique de contrôle… renonciation HeadlessAppBypass » | une seconde `<Application>` portait `AppListEntry="none"` pour exposer `docker` sans l'afficher au menu Démarrer | l'alias d'exécution porte son propre `Executable` : il est déclaré sous l'application principale, et la seconde application supprimée |
| « The Extension element with Category "windows.appExecutionAlias" must only be declared once » | deux alias déclarés sous la même application | un seul alias, `docker` ; l'alias `solon` est abandonné |
| « fonctionnalités restreintes nécessitent une approbation » | `runFullTrust`, `packagedServices`, `localSystemServices`, `allowElevation` | avertissement attendu, pas bloquant : la justification est dans les notes de certification (`.local/build/store-listing.md`) |

## Ce qui peut coincer

- **Les capacités restreintes.** C'est le vrai risque : aucun gestionnaire de conteneurs n'est
  aujourd'hui sur le Store, et Microsoft peut estimer qu'un service système sort du cadre. Le refus
  éventuel sera motivé, et le paquet MSIX reste utile hors Store : signé avec un certificat de
  l'entreprise, il s'installe proprement en environnement géré.
- **La version.** Le Store se réserve la dernière partie du numéro : `0.1.13.0` passe, `0.1.13.2`
  non. Le script le vérifie. Une soumission ne peut pas reprendre un numéro déjà utilisé, même
  retiré.
- **Le poids.** 136 Mo compressés, 400 Mo installés, dont l'image Linux. C'est sous les limites du
  Store, mais chaque version republie le tout.
- **La cohabitation.** Un PC ne peut pas avoir les deux installations à la fois. Si Solon arrive sur
  le Store, il faudra dire dans les notes de version qu'on désinstalle l'une avant l'autre, et que
  les données sont conservées entre les deux.
