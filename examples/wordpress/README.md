# Exemple WordPress

WordPress (dernière version) et MariaDB 11, deux volumes Docker (`db`, `html`).

1. Dans Solon : Containers → **Open a project…** → ce dossier → **Up**. Ou en terminal :
   `docker compose -f examples\wordpress\compose.yaml up -d`.
2. Ouvrir **http://wordpress.wordpress.solon.local/** (ou `http://localhost:8080`) et suivre l'installation
   WordPress (langue, titre, compte administrateur).
3. Pour développer un thème ou une extension depuis Windows, décommenter le montage `./wp-content` dans
   `compose.yaml` et relancer `Up` : le dossier apparaît à côté de ce fichier.

Pour ce test, restez en **http** : le mandataire de Solon ne transmet pas encore l'en-tête qui dit à WordPress
qu'il est servi en HTTPS ; en https il générerait des liens http (contenu mixte).

`Down` arrête et supprime les conteneurs ; les volumes restent (supprimez-les dans Volumes pour repartir de zéro).
