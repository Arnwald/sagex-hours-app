# sagex-hours

Saisie des heures HES-SO / SageX : une base JSON en clair sur le disque, une API
HTTP, une interface web, et l'export du `.xlsx` attendu par *Heures → Transfert
des heures*.

Remplace le couple Toggl + [`sagex-rs`](https://gitlab.hevs.ch/SPL/sagex/sagex-rs) :
les heures se saisissent ici, en un clic ou en dictant à Claude, et l'export
reproduit exactement le format que SageX ingère déjà.

## Ce que ça fait

- **Vue semaine** : grille horaire, création par glissé, déplacement et
  redimensionnement des blocs, tâches en un clic, bande « sans horaire » pour les
  saisies sans heure précise.
- **Vue mois** : totaux par projet, par activité et par jour, écart avec
  l'attendu (jours ouvrables × 8h12 × taux d'activité, fériés valaisans calculés),
  contrôles avant envoi, aperçu ligne à ligne et téléchargement du xlsx.
- **Fenêtre de gestion** : projets SageX, activités SageX et tâches — trois listes
  indépendantes, comme dans SageX, plus les réglages et la sauvegarde.
- **Chronomètre** start/stop.
- **Base pilotable** : une modification par l'API ou à même les fichiers JSON
  apparaît instantanément dans le navigateur.
- **Import Toggl** : rapatrie l'historique (jusqu'à septembre 2024 via la Reports
  API) en conservant les horaires réels.

## Démarrer

```bash
cargo run -- serve            # http://localhost:8080
```

Première mise en route depuis une configuration `sagex-rs` existante :

```bash
cargo run -- import-ron /chemin/vers/sagex-rs.ron
cargo run -- import-toggl --month 2026-07 --dry-run
```

`import-ron` éclate la configuration en trois listes (les activités, dupliquées
dans chaque projet du RON, sont dédoublonnées) et reprend les numéros de
collaborateur, de personne et d'école dans `settings.json`.

## La base

```
data/
  settings.json          identité SageX, taux d'activité, options d'export, jeton Toggl
  catalog.json           projects[] · activities[] · presets[]
  entries/2026-09.json   les saisies du mois
  backups/               archives zip (dont les sauvegardes de sécurité avant restauration)
```

Tout est lisible et modifiable à la main. Le serveur surveille le dossier :
éditer un fichier met l'interface à jour sans rechargement.

Une saisie :

```json
{
  "id": "01M2MJND8JRRPEFHPM9NH9RBC3",
  "date": "2026-09-14",
  "start": "09:00", "end": "12:00", "minutes": 180,
  "project_id": "subice-marvis-fns-sefri",
  "activity_id": "ra-d",
  "comment": "Biblio diffraction",
  "source": "manual",
  "created_at": "2026-09-14T09:02:11", "updated_at": "2026-09-14T09:02:11"
}
```

`start` et `end` sont facultatifs ; `minutes` fait foi.

## Ligne de commande

```bash
sagex-hours serve [--port 8080] [--host 0.0.0.0]
sagex-hours import-ron <fichier.ron> [--dry-run] [--force]
sagex-hours import-toggl --month 2026-07 [--dry-run]
sagex-hours import-toggl --from 2026-07-01 --to 2026-07-15
sagex-hours export 2026-07 [--out fichier.xlsx] [--force]
```

`--data <dossier>` (ou `SAGEX_DATA_DIR`) choisit la base.

## API

Tout est en JSON ; les références acceptent l'identifiant, le numéro SageX ou le
nom, et les durées s'écrivent `1h30`, `90`, `1.5h`…

```bash
# Ajouter des heures
curl -X POST localhost:8080/api/entries -H 'content-type: application/json' -d '
  {"date":"2026-09-16","project":"Subice","activity":"ra-d","duration":"4h","comment":"Modèle"}'

# Recopier une semaine sur la suivante, sans le week-end
curl -X POST localhost:8080/api/entries/copy -H 'content-type: application/json' -d '
  {"mode":"week","source":"2026-09-14","target":"2026-09-21","skip_weekend":true}'

# Contrôles et totaux du mois
curl localhost:8080/api/month/2026-09/report

# Fichier d'import SageX
curl -o septembre.xlsx localhost:8080/api/export/2026-09
```

Le détail des routes et des formulations acceptées est dans [CLAUDE.md](CLAUDE.md).

## Export vers SageX

Les colonnes reproduisent celles de `sagex-rs`, vérifiées sur les fichiers déjà
importés : n° école, n° collaborateur, n° projet, n° activité, heures `HHhMM`,
colonne « centième » vide, date en série Excel au format `dd.mm.yyyy`,
commentaire, `I_PERSONNE`.

Trois agrégations : une ligne par projet et par mois (datée du dernier jour du
mois, comme l'exige la directive), par jour, ou une ligne par saisie.

La colonne « Commentaire » est un `varchar1000`. Le style **auto** détaille les
saisies tant qu'on reste sous la limite, puis regroupe les descriptions
identiques — `Draft biblio (12× : 31.07, 30.07, …)`. Sans cela, un mois chargé
dépasse la limite et SageX tronque en silence.

> Le `.xlsx` doit encore être ré-enregistré en `.xls` depuis Excel avant l'import
> dans SageX, comme avec `sagex-rs`.

## Contrôles

Avant export, le mois est vérifié : identité SageX renseignée, références de
projet ou d'activité valides, commentaire sous la limite, dates dans le mois,
chevauchements d'horaires, journées anormalement longues, écart avec l'attendu.
Les erreurs bloquent le téléchargement (contournable avec `force`), le reste est
signalé.

## Sauvegarde

`GET /api/backup` produit un zip de toute la base et note la date ; passé le
délai configuré (7 jours par défaut), l'interface propose la sauvegarde à
l'ouverture. La restauration copie d'abord l'état courant dans `backups/`.

## Déploiement

```bash
docker compose up -d --build
```

Le binaire embarque l'interface ; seul `/data` est à monter. Sur TrueNAS SCALE,
monter un dataset sur `/data`, aligner `user:` sur son propriétaire, et
**définir `SAGEX_TOKEN`** dès que l'app est joignable hors du réseau local.

## Développement

```bash
cargo test          # modèle, base, conversion RON, durées, export, contrôles
cargo run -- serve  # en debug, le dossier web/ est relu à chaque requête
```
