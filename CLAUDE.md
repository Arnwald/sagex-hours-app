# sagex-hours — mémo de pilotage

App de saisie des heures SageX. Base JSON en clair dans `data/`, API HTTP, UI web.

## Deux façons d'agir sur la base

1. **L'API** (`http://localhost:8080`, ou l'adresse du NAS) — la seule qui marche
   à distance, et celle à privilégier : elle valide les références et les durées.
2. **Les fichiers** `data/*.json` en édition directe — le serveur surveille le
   dossier et l'interface se met à jour toute seule. Pratique pour une reprise en
   masse, mais rien n'est validé.

Si l'instance exige un jeton : `-H "X-Auth-Token: $SAGEX_TOKEN"`.

**Avant toute chose**, vérifier que le serveur répond :
`curl -s -m 2 localhost:8080/api/settings`. S'il ne répond pas, le démarrer
depuis la racine du projet : `cargo run --quiet -- serve` (il reste au
premier plan — le lancer en tâche de fond). Instance distante : remplacer
`localhost:8080` par son adresse dans toutes les commandes ci-dessous.

## Les trois listes

- **projects** — projets SageX : `{id, sagex_number, name, color, exportable, archived}`.
  `exportable: false` → visible dans l'app, absent du xlsx (congés, récupération).
- **activities** — activités SageX, liste unique valable pour tous les projets.
- **presets** (« tâches ») — raccourcis `{id, label, project_id, activity_id, comment, toggl}`.

`GET /api/catalog` donne les identifiants à utiliser.

## Saisir des heures

`POST /api/entries` accepte un objet ou un tableau. Les références acceptent
l'identifiant, le numéro SageX ou le nom (même partiel) ; les durées s'écrivent
`1h30`, `1:30`, `90`, `90min`, `1.5h` ; les dates `2026-09-16`, `16.09.2026`,
`hier`, `aujourd'hui`.

```bash
curl -X POST localhost:8080/api/entries -H 'content-type: application/json' -d '[
  {"date":"2026-09-16","preset":"Daily","start":"08:30","duration":"30m"},
  {"date":"2026-09-16","project":"Subice","activity":"ra-d","start":"09:00","end":"12:00","comment":"Biblio"},
  {"date":"2026-09-17","project":129030,"activity":29,"hours":4,"comment":"Modèle"}
]'
```

`preset` remplit projet + activité + commentaire ; `project` et `activity`
l'emportent s'ils sont aussi fournis. Sans horaire, la saisie atterrit dans la
bande « sans horaire » du jour — parfaitement valable.

Une durée est obligatoire : `duration`, `minutes`, `hours`, ou `start` + `end`.

## Modifier, supprimer, dupliquer

```bash
# Modification partielle (null efface un horaire)
curl -X PATCH localhost:8080/api/entries/<id> -H 'content-type: application/json' \
  -d '{"duration":"2h","comment":"Rédaction du rapport"}'

curl -X DELETE localhost:8080/api/entries/<id>

# Dupliquer un jour vers plusieurs jours
curl -X POST localhost:8080/api/entries/copy -H 'content-type: application/json' \
  -d '{"mode":"day","source":"2026-09-14","targets":["2026-09-15","2026-09-16"]}'

# Dupliquer une semaine entière (source et cible : n'importe quel jour de la semaine)
curl -X POST localhost:8080/api/entries/copy -H 'content-type: application/json' \
  -d '{"mode":"week","source":"2026-09-14","target":"2026-09-21","skip_weekend":true,"replace":true}'

# Vider une période — toujours vérifier avec dry_run d'abord
curl -X POST localhost:8080/api/entries/clear -H 'content-type: application/json' \
  -d '{"from":"2026-09-14","to":"2026-09-18","dry_run":true}'
```

`replace: true` vide la cible avant de copier. `clear` accepte `project` pour se
limiter à un projet.

## Lire

```bash
curl "localhost:8080/api/state?from=2026-09-14&to=2026-09-20"   # catalogue + saisies + fériés
curl "localhost:8080/api/entries?from=2026-09-01&to=2026-09-30"
curl localhost:8080/api/month/2026-09/report                     # totaux, écart, contrôles, aperçu
curl localhost:8080/api/settings
```

Le rapport mensuel est ce qu'il faut regarder avant de proposer un export :
`issues[]` liste les erreurs (bloquantes) et les avertissements, avec le code
(`commentaire-trop-long`, `chevauchement`, `hors-mois`, `reference-inconnue`,
`total-inattendu`, `journee-longue`, `duree-nulle`, `settings-incomplets`).

## Catalogue et réglages

```bash
curl -X POST localhost:8080/api/catalog/projects -H 'content-type: application/json' \
  -d '{"name":"SEfAH - No 133.981 INNO-EE","sagex_number":142718}'
curl -X POST localhost:8080/api/catalog/presets -H 'content-type: application/json' \
  -d '{"label":"Ra&D SEfAH","project":"142718","activity":"ra-d"}'
curl -X PATCH localhost:8080/api/catalog/activities/<id> -H 'content-type: application/json' -d '{"archived":true}'
curl -X PATCH localhost:8080/api/settings -H 'content-type: application/json' -d '{"activity_rate":0.9}'
```

Supprimer un projet ou une activité encore utilisé renvoie 409 : archiver, ou
`?force=true` en connaissance de cause.

## Export et sauvegarde

```bash
curl -o septembre.xlsx "localhost:8080/api/export/2026-09?aggregate=month&comment_style=auto"
curl -o base.zip localhost:8080/api/backup
curl -X POST --data-binary @base.zip "localhost:8080/api/restore?dry_run=true"
```

L'export refuse de produire un fichier si le mois comporte des erreurs :
`&force=true` passe outre. Le xlsx doit être ré-enregistré en `.xls` depuis Excel
avant l'import dans SageX.

## Import Toggl

```bash
curl -X POST localhost:8080/api/import/toggl -H 'content-type: application/json' \
  -d '{"month":"2026-07","dry_run":true}'
```

Toujours commencer par `dry_run`. Le résumé indique les couples client/projet
Toggl sans tâche correspondante : il faut créer les tâches manquantes avant de
relancer. L'import est rejouable — les saisies déjà rapatriées sont reconnues à
leur `toggl_id`.

## Règles métier à garder en tête

- 8h12 par jour pour un 100 % (directive SageX 2026), fériés valaisans calculés.
- La colonne « Commentaire » est un `varchar1000` : au-delà, SageX tronque.
- Une ligne mensuelle porte la date du **dernier jour du mois**.
- Les heures du mois précédent se saisissent pendant la première semaine du mois
  courant.
- Un projet dont `sagex_number` vaut 0 ou `exportable: false` ne part jamais dans
  le xlsx.

## Code

`src/model.rs` types de la base · `src/store.rs` lecture/écriture atomique ·
`src/export.rs` lignes SageX et xlsx · `src/validate.rs` contrôles et fériés ·
`src/parse.rs` durées et résolution des références · `src/api/` routes ·
`src/toggl.rs` import · `web/` interface (modules ES, sans build).

`cargo test` couvre le modèle, la base, la conversion RON, les durées, l'export
et les contrôles.
