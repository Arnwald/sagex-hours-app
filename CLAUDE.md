# sagex-hours — operating notes

SageX hours app. A plain JSON base under `data/`, an HTTP API, an MCP endpoint,
a web interface in French or English.

## Three ways to act on a base

1. **The MCP tools** (`/mcp`), when this session has them: the same checks as
   the API, as the person whose key they carry.
2. **The API** (`http://localhost:8080`, or the server's address) — works
   remotely, and validates references and durations.
3. **The files** in direct edit — the server watches the folder and the
   interface updates itself. Handy for bulk changes, but nothing is validated.

Where the files are: single-user, at the root of `data/`; with several users
(`data/users.json` exists), in `data/users/<id>/`.

With a key: `-H "Authorization: Bearer $SAGEX_TOKEN"` (or `X-Auth-Token`). With
several users the key also says whose base the calls touch.

**First of all**, check the server answers:
`curl -s -m 2 localhost:8080/api/me`. If it doesn't, start it from the project's
root: `cargo run --quiet -- serve` (it stays in the foreground — run it in the
background). Remote instance: replace `localhost:8080` with its address in
every command below.

## The three lists

- **projects** — SageX projects: `{id, sagex_number, name, color, exportable, archived}`.
  `exportable: false` → visible in the app, left out of the xlsx (holidays, time off in lieu).
- **activities** — SageX activities, one list valid for every project.
- **presets** ("tasks") — shortcuts `{id, label, project_id, activity_id, comment, toggl}`.

`GET /api/catalog` gives the identifiers to use.

## Entering hours

`POST /api/entries` takes an object or an array. References accept the
identifier, the SageX number or the name (even partial); durations are written
`1h30`, `1:30`, `90`, `90min`, `1.5h`; dates `2026-09-16`, `16.09.2026`,
`yesterday`, `today` (also in French: `hier`, `aujourd'hui`).

```bash
curl -X POST localhost:8080/api/entries -H 'content-type: application/json' -d '[
  {"date":"2026-09-16","preset":"Daily","start":"08:30","duration":"30m"},
  {"date":"2026-09-16","project":"Subice","activity":"ra-d","start":"09:00","end":"12:00","comment":"Bibliography"},
  {"date":"2026-09-17","project":129030,"activity":29,"hours":4,"comment":"Model"}
]'
```

`preset` fills project + activity + comment; `project` and `activity` win if
they are given too. Without times, the entry lands in the day's "untimed" band —
perfectly valid.

A duration is required: `duration`, `minutes`, `hours`, or `start` + `end`.

## Changing, deleting, copying

```bash
# Partial change (null clears a time)
curl -X PATCH localhost:8080/api/entries/<id> -H 'content-type: application/json' \
  -d '{"duration":"2h","comment":"Writing the report"}'

curl -X DELETE localhost:8080/api/entries/<id>

# Copy a day to several days
curl -X POST localhost:8080/api/entries/copy -H 'content-type: application/json' \
  -d '{"mode":"day","source":"2026-09-14","targets":["2026-09-15","2026-09-16"]}'

# Copy a whole week (source and target: any day of the week)
curl -X POST localhost:8080/api/entries/copy -H 'content-type: application/json' \
  -d '{"mode":"week","source":"2026-09-14","target":"2026-09-21","skip_weekend":true,"replace":true}'

# Clear a period — always check with dry_run first
curl -X POST localhost:8080/api/entries/clear -H 'content-type: application/json' \
  -d '{"from":"2026-09-14","to":"2026-09-18","dry_run":true}'
```

`replace: true` empties the target before copying. `clear` takes `project` to
limit itself to one project.

## Reading

```bash
curl "localhost:8080/api/state?from=2026-09-14&to=2026-09-20"   # catalogue + entries + holidays
curl "localhost:8080/api/entries?from=2026-09-01&to=2026-09-30"
curl localhost:8080/api/month/2026-09/report                     # totals, gap, checks, preview
curl localhost:8080/api/settings
```

The month report is what to look at before suggesting an export: `issues[]`
lists the errors (blocking) and the warnings, with their code
(`comment-too-long`, `overlap`, `outside-month`, `unknown-reference`,
`not-exported`, `unexpected-total`, `long-day`, `zero-duration`,
`incomplete-settings`).

## Catalogue and settings

```bash
curl -X POST localhost:8080/api/catalog/projects -H 'content-type: application/json' \
  -d '{"name":"SEfAH - No 133.981 INNO-EE","sagex_number":142718}'
curl -X POST localhost:8080/api/catalog/presets -H 'content-type: application/json' \
  -d '{"label":"Ra&D SEfAH","project":"142718","activity":"ra-d"}'
curl -X PATCH localhost:8080/api/catalog/activities/<id> -H 'content-type: application/json' -d '{"archived":true}'
curl -X PATCH localhost:8080/api/settings -H 'content-type: application/json' -d '{"activity_rate":0.9}'
```

Deleting a project or an activity still in use returns 409: archive it, or
`?force=true` knowingly.

## Export and backup

```bash
curl -o september.xlsx "localhost:8080/api/export/2026-09?aggregate=month&comment_style=auto"
curl -o base.zip localhost:8080/api/backup
curl -X POST --data-binary @base.zip "localhost:8080/api/restore?dry_run=true"
```

The export refuses to produce a file if the month has errors: `&force=true`
overrides. The xlsx must be re-saved as `.xls` from Excel before the SageX
import.

## Toggl import

```bash
curl -X POST localhost:8080/api/import/toggl -H 'content-type: application/json' \
  -d '{"month":"2026-07","dry_run":true}'
```

Always start with `dry_run`. The summary lists the Toggl client/project pairs
without a matching task: create the missing tasks before running it again. The
import can be replayed — entries already brought back are recognised by their
`toggl_id`.

## Users (administrators)

```bash
curl localhost:8080/api/me                                          # who the key opens
curl localhost:8080/api/admin/users
curl -X POST localhost:8080/api/admin/users -H 'content-type: application/json' -d '{"name":"Alice Martin"}'
curl -X POST localhost:8080/api/admin/users/<id>/key                # new key
curl -X DELETE localhost:8080/api/admin/users/<id>                  # key revoked, folder kept
curl -X POST localhost:8080/api/admin/enable-users -H 'content-type: application/json' -d '{"name":"…"}'
```

A new user's key is in the answer, and only there.

## Business rules to keep in mind

- 8h12 a day for a 100 % position (SageX 2026 directive), Valais public holidays computed.
- The « Commentaire » column is a `varchar1000`: beyond it, SageX cuts.
- A monthly row carries the date of the **last day of the month**.
- The previous month's hours are entered during the first week of the current
  month.
- A project whose `sagex_number` is 0 or `exportable: false` never goes into
  the xlsx.

## Code

`src/model.rs` the base's types · `src/store.rs` atomic reads and writes ·
`src/users.rs` users and keys · `src/app.rs` one base or one per user, access ·
`src/export.rs` SageX rows and xlsx · `src/validate.rs` checks and holidays ·
`src/parse.rs` durations and references · `src/api/` routes · `src/mcp.rs` the
MCP endpoint and its tools · `src/api/claude.rs` and `mcpb/` the Claude Desktop
extension · `src/i18n.rs` the server's messages in English and French ·
`src/toggl.rs` import · `web/` interface (ES modules, no build; its texts in
`web/js/i18n.js`).

Rules: every message the server returns goes through `tr!("English text")`
with its French version in `src/i18n.rs`; every interface text has its key in
both languages of `web/js/i18n.js`; code, comments and docs in English.

`cargo test` covers the model, the base, users, translations, the RON
conversion, durations, the export, the checks and the MCP tools.
