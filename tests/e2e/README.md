# Tests de bout en bout du service

Ces scripts exercent le vrai service (`solon-service`) contre une vraie machine. Ils exigent une machine
Windows Pro/Entreprise avec Hyper-V et la Plateforme de machine virtuelle activés, et une image construite
(`image/build.sh`). Ils ne sont pas lancés par `cargo test` : seul le lancement du service demande une
élévation (UAC) ; tout le reste tourne **sans droits**, comme l'application.

| Script | Rôle | Élévation |
|---|---|---|
| `start-console.ps1 -ImageDir <dossier>` | Lance `solon-service console --start` élevé, journaux dans `.local/build/service-console.log` | UAC ×1 |
| `e2e.ps1` | Scénario complet : prérequis, état, `docker version`, `pull`, port publié → `localhost`, RAM au repos, arrêt propre | aucune |
| `crash-force-stop.ps1` | Terminaison brutale de la machine pendant des écritures, redémarrage, `fsck`, données conservées | aucune |
| `crash-service-kill.ps1` | Service tué pendant que la machine tourne, relance, rattachement sans perte des conteneurs | UAC ×2 |
| `quit.ps1` | Arrête le moteur puis le service console | aucune |

Prérequis côté CLI Docker : `DOCKER_HOST=npipe:////./pipe/solon`. Si Docker Desktop est installé, le CLI
utilise le gestionnaire d'identifiants Windows et peut envoyer des identifiants Docker Hub périmés
(« unauthorized ») : les scripts tirent leurs images depuis `public.ecr.aws` pour rester indépendants.

Résultats et chiffres : `docs/measurements.md`, section « Bloc 2 ».

- `install-test.ps1` : installe Solon en silence depuis l'installeur NSIS (une fenêtre UAC), vérifie le service Windows réel, démarre le moteur puis enchaîne `e2e.ps1`. Ne désinstalle pas.

- `fresh-machine-phase1.ps1` (élevé) / `fresh-machine-phase2.ps1` : test « machine vierge » : retire Docker Desktop, WSL, Solon et désactive Hyper-V (phase 1, puis redémarrage, installation de Solon, redémarrage), puis vérifie Solon seul (phase 2).
