# sagex-hours

HES-SO / SageX hours: a plain JSON base on disk, an HTTP API, a web interface
in French or English, an MCP endpoint for Claude, and the `.xlsx` that
*Heures → Transfert des heures* expects. One instance serves a whole team, each
person with their own base and personal key.

It replaces the Toggl + [`sagex-rs`](https://gitlab.hevs.ch/SPL/sagex/sagex-rs)
pair: hours are entered here, in one click or by talking to Claude, and the
export reproduces exactly the format SageX already ingests.

## What it does

- **Week view**: time grid, drag to create, move and resize blocks, tasks in
  one click, an "untimed" band for entries without a precise time.
- **Month view**: totals per project, per activity and per day, the gap with
  the expected hours (working days × 8h12 × activity rate, Valais public
  holidays computed), checks before sending, a row-by-row preview and the
  xlsx download.
- **Manage window**: SageX projects, SageX activities and tasks — three
  independent lists, as in SageX —, the settings and the backup, how to
  connect Claude, and the users (for administrators).
- **French or English**, switched at any time (FR | EN), remembered per
  browser; the first choice follows the browser's language. The server's
  messages (errors, checks, holidays) follow too.
- **Several people**: one instance, a base and a personal key each, sent as an
  invitation link.
- **Claude**: an extension for Claude Desktop downloaded from the app (a
  double-click, nothing else to install), one command line for Claude Code.
- **Timer** start/stop.
- **Live base**: a change through the API, Claude or the JSON files appears at
  once in the browser.
- **Toggl import**: brings back the history (up to September 2024 through the
  Reports API) with the actual times.

## Getting started

```bash
cargo run -- serve            # http://localhost:8080
```

First run from an existing `sagex-rs` setup:

```bash
cargo run -- import-ron /path/to/sagex-rs.ron
cargo run -- import-toggl --month 2026-07 --dry-run
```

`import-ron` splits the configuration into three lists (the activities, repeated
in every project of the RON, are deduplicated) and copies the employee, person
and school numbers into `settings.json`.

## The base

```
data/
  users.json                        the users (only with several users)
  users/<id>/                       one person's base:
    settings.json                     SageX identity, activity rate, export options, Toggl token
    catalog.json                      projects[] · activities[] · presets[]
    entries/2026-09.json              the month's entries
    backups/                          zip archives (incl. safety copies taken before a restore)
```

A single-user instance keeps the base's files at the root of `data/`, without
`users.json`. Everything can be read and edited by hand. The server watches
the folder: editing a file updates the interface without reloading.

An entry:

```json
{
  "id": "01M2MJND8JRRPEFHPM9NH9RBC3",
  "date": "2026-09-14",
  "start": "09:00", "end": "12:00", "minutes": 180,
  "project_id": "subice-marvis-fns-sefri",
  "activity_id": "ra-d",
  "comment": "Diffraction bibliography",
  "source": "manual",
  "created_at": "2026-09-14T09:02:11", "updated_at": "2026-09-14T09:02:11"
}
```

`start` and `end` are optional; `minutes` is authoritative.

## Several users

One instance, one base per person: no container per colleague.

1. **Enable**: *Manage › Users › Enable several users* (or
   `sagex-hours users enable --name "…"`). The current base becomes yours, as
   administrator, and the current `SAGEX_TOKEN` stays your key: browsers and
   Claude set up with it go on working.
2. **Invite**: *Manage › Users*, a name, *Add*. The new base starts from your
   SageX activities (and your projects if ticked); tasks stay personal. Send
   the invitation link shown — `http://<server>/#key=…`: it opens their hours
   and keeps the key in their browser. It is shown once; *New link* makes
   another and voids the old one.
3. **Remove**: takes the person off `users.json`; their key stops working,
   their folder stays on the server.

Keys are 48 hex characters; `users.json` keeps only their SHA-256. Each person
sees only their own hours in the app; the administrator of the server has the
files. The command line does the same as the page:

```bash
sagex-hours users list
sagex-hours users add "Alice Martin" [--admin] [--with-projects]
sagex-hours users key alice-martin        # new key
sagex-hours users remove alice-martin
```

In Docker: `docker exec -it sagex-hours sagex-hours users …`; the running
server picks the change up from `users.json`.

## Claude (MCP)

The server speaks MCP at `/mcp` (Streamable HTTP). Its tools call the API as
the person whose key they carry, with the same checks: add, change, copy and
clear entries, the month report, the catalogue, the settings, the timer, the
Toggl import. The server's instructions give Claude the SageX rules (8h12,
varchar1000, monthly rows dated the last day…). The computer running Claude
must reach the server (the VPN).

- **Claude Desktop**: *Manage › Claude › Download the extension*, then
  double-click `sagex-hours.mcpb` › *Install*. The extension is made for the
  person who downloads it: the server's address and their key are inside, and
  Claude Desktop runs its small relay (`mcpb/server/index.js`, no dependency)
  with its own Node.js. The tools stay on the server: updating the server
  updates them for everyone.
- **Claude Code**: *Manage › Claude* gives the line to paste once:

  ```bash
  claude mcp add --transport http --scope user sagex-hours http://<server>/mcp \
    --header "Authorization: Bearer <key>"
  ```

claude.ai's custom connectors call the server from Anthropic's cloud, so they
need an instance reachable from the internet over HTTPS; behind a VPN, use the
two ways above.

## Command line

```bash
sagex-hours serve [--port 8080] [--host 0.0.0.0]
sagex-hours import-ron <file.ron> [--dry-run] [--force]
sagex-hours import-toggl --month 2026-07 [--dry-run]
sagex-hours import-toggl --from 2026-07-01 --to 2026-07-15
sagex-hours export 2026-07 [--out file.xlsx] [--force]
sagex-hours users list|enable|add|key|remove
```

`--data <folder>` (or `SAGEX_DATA_DIR`) picks the data folder; with several
users, `--user <id>` picks whose base. `--lang fr` gives the checks' messages
and the export's generated text in French.

## API

Everything is JSON. References accept the identifier, the SageX number or the
name, and durations are written `1h30`, `90`, `1.5h`… With a key:
`-H "Authorization: Bearer <key>"` (or `X-Auth-Token`, or `?token=`). Messages
are in English unless `?lang=fr` or `Accept-Language: fr`.

```bash
# Add hours
curl -X POST localhost:8080/api/entries -H 'content-type: application/json' -d '
  {"date":"2026-09-16","project":"Subice","activity":"ra-d","duration":"4h","comment":"Model"}'

# Copy a week onto the next, without the weekend
curl -X POST localhost:8080/api/entries/copy -H 'content-type: application/json' -d '
  {"mode":"week","source":"2026-09-14","target":"2026-09-21","skip_weekend":true}'

# A month's checks and totals
curl localhost:8080/api/month/2026-09/report

# The SageX import file
curl -o september.xlsx localhost:8080/api/export/2026-09
```

The routes and accepted spellings are detailed in [CLAUDE.md](CLAUDE.md).

## Export to SageX

The columns reproduce those of `sagex-rs`, checked against files already
imported: school number, employee number, project number, activity number,
hours `HHhMM`, an empty "hundredths" column, the date as an Excel serial
formatted `dd.mm.yyyy`, the comment, `I_PERSONNE`.

Three aggregations: one row per project per month (dated the last day of the
month, as the directive requires), per day, or one row per entry.

The comment column is a `varchar1000`. The **auto** style lists the entries
while it stays under the limit, then groups identical descriptions —
`Draft biblio (12×: 31.07, 30.07, …)`. Without it, a busy month goes over the
limit and SageX silently cuts the text.

> The `.xlsx` must still be re-saved as `.xls` from Excel before the SageX
> import, as with `sagex-rs`.

## Checks

Before the export, the month is checked: SageX identity filled in, valid project
and activity references, comments under the limit, dates within the month,
overlapping times, unusually long days, the gap with the expected hours. Errors
block the download (overridable with `force`); the rest is reported.

## Backup

`GET /api/backup` makes a zip of one's whole base and notes the date; after the
configured delay (7 days by default), the interface offers the backup when it
opens. A restore first copies the current state into `backups/`.

## Deployment (Linux server with Docker)

A push to `main` runs [`image.yml`](.github/workflows/image.yml), which publishes
`ghcr.io/arnwald/sagex-hours-app:latest`.

1. **Image** — the repository is private, so is the image: on the server,
   `docker login ghcr.io -u <account> -p <token with read:packages>` (or make
   the package public in *Packages › Package settings*).
2. **Stack** — copy [`deploy/compose.yaml`](deploy/compose.yaml) to the server,
   adapt the port, the data folder and `SAGEX_TOKEN`, then `docker compose up -d`.
   Update with `docker compose pull && docker compose up -d`.
3. **Bring the base over** — on the old instance, *Manage › Settings › Download
   the database*; on the new one, *Restore* with that zip.
4. **Open it to colleagues** — *Manage › Users › Enable several users*, then
   one invitation link per colleague.

`SAGEX_TOKEN` is not optional as soon as the app is reachable beyond one's own
computer: without it a single-user instance is open for reading and writing.
Over a VPN, plain HTTP is acceptable; elsewhere, put a reverse proxy with TLS in
front.

To build without a registry (the repository cloned on the server):
`docker compose up -d --build` with the [`docker-compose.yml`](docker-compose.yml)
at the root. It is heavier — Rust compiles on the server at every update.

## Development

```bash
cargo test          # model, base, users, translations, durations, export, checks, MCP
cargo run -- serve  # in debug builds, web/ is read again on every request
```

Server messages are written in English with `tr!("…")`; their French version
goes in the table of `src/i18n.rs` (a test checks that none is missing). The
interface's texts are in `web/js/i18n.js`, French and English side by side.
