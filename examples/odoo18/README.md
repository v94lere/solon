# Exemple : Odoo 18 Community + PostgreSQL 16

1. Dans Solon, onglet Conteneurs → « Ouvrir un projet… » → choisir ce dossier, puis **Up**.
2. Ouvrir http://localhost:8069, créer une base (mot de passe maître : `admin`, à changer dans `config/odoo.conf`).
3. Les modules personnalisés vont dans `addons/` ; les données (base, pièces jointes) sont dans des volumes Docker,
   donc à vitesse native.

Mesuré sur la machine de développement (voir `docs/measurements.md`, « Comparatif Docker Desktop ») : pile prête en
~10 s, création d'une base avec données de démonstration en ~16 s.
