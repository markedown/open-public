# Database backup operations

This guide covers `scripts/backup_db.sh` and `scripts/restore_check.sh`.
All hosts, container names and paths below are placeholders; substitute your
own.

## Overview

Backups are taken with the Postgres container's own `pg_dump`, never a client
installed on the host. Client and server major versions can differ, and a
mismatched `pg_dump` can produce an empty archive that looks like a successful
backup right up to the moment it is needed. For the same reason a dump is only
kept after it verifies: the script writes to a `.partial` file, checks its size
and that the container's `pg_restore --list` can read it, and only then renames
it into the retention pool. A dump that fails either check is discarded and the
script exits non-zero.

## Taking a backup

The script is configured entirely by environment variables, so it runs
unmodified from cron or a deployment hook.

| Variable | Required | Default | Meaning |
| --- | --- | --- | --- |
| `CONTAINER` | yes | | docker container running Postgres |
| `BACKUP_DIR` | yes | | directory the dumps are written to |
| `DB_USER` | no | `open_public` | database role to dump as |
| `DB_NAME` | no | `open_public` | database to dump |
| `KEEP_DAYS` | no | `14` | days of dump history to retain |
| `UPLOAD_CMD` | no | unset | shell command that ships the dump off-box |

Example invocation:

```bash
CONTAINER=open-public-postgres BACKUP_DIR=/opt/open-public/backups \
  /opt/open-public/scripts/backup_db.sh
```

The dump is a `pg_dump -Fc` (custom format) archive named
`$DB_NAME-<UTC timestamp>.dump`, where the timestamp is
`%Y%m%dT%H%M%SZ`. For example:
`open_public-20261102T030405Z.dump`.

The script sets `umask 077` before creating anything, so the dump file, and
`BACKUP_DIR` when the script has to create it, are readable only by their
owner, whatever the caller's umask is. An existing `BACKUP_DIR` keeps its own
permissions, so restrict it yourself. A
dump holds every row in the database, including user accounts and their
password hashes, so it must not be world-readable.

## Retention

After a dump is verified and renamed, the script deletes older dumps in
`BACKUP_DIR` whose modification time is more than `KEEP_DAYS` days old
(`find -maxdepth 1 -name "$DB_NAME-*.dump" -mtime "+$KEEP_DAYS"`). `KEEP_DAYS`
must be a non-negative integer; anything else aborts the run rather than
silently turning into a wrong deletion window.

Never pruned: the dump just written, files in subdirectories of `BACKUP_DIR`,
and anything not matching `$DB_NAME-*.dump`. That last point includes
`.partial` files left behind by a run killed before its own cleanup could
run; they are never matched by the prune pattern and must be removed by hand.

## Shipping a dump off the machine

If `UPLOAD_CMD` is set, it is run through `sh` with the dump path passed as
positional argument `$1`, so multi-word commands and paths with spaces both
work. Two generic examples:

```bash
# rsync the dump to another host
UPLOAD_CMD='rsync -a "$1" backup.example.org:/srv/backups/open-public/'

# a script that reads the dump path as its last argument
UPLOAD_CMD=/opt/open-public/scripts/ship_dump.sh
```

A failed upload leaves the verified local dump untouched and makes the script
exit non-zero. Pruning has already run by then, so the dump is still on disk
and can be shipped by hand or on the next run.

## Scheduling

A cron entry:

```cron
0 3 * * * CONTAINER=open-public-postgres BACKUP_DIR=/opt/open-public/backups \
  /opt/open-public/scripts/backup_db.sh >> /var/log/open-public-backup.log 2>&1
```

The equivalent systemd timer, a service and a timer unit:

```ini
# /etc/systemd/system/open-public-backup.service
[Unit]
Description=Back up the open-public database

[Service]
Type=oneshot
Environment=CONTAINER=open-public-postgres
Environment=BACKUP_DIR=/opt/open-public/backups
ExecStart=/opt/open-public/scripts/backup_db.sh
```

```ini
# /etc/systemd/system/open-public-backup.timer
[Unit]
Description=Daily open-public database backup

[Timer]
OnCalendar=*-*-* 03:00:00
Persistent=true

[Install]
WantedBy=timers.target
```

Enable it with `systemctl enable --now open-public-backup.timer`.

## Restore check

`scripts/restore_check.sh` proves a dump can actually be restored. Point it at
a dump file:

```bash
CONTAINER=open-public-postgres \
  /opt/open-public/scripts/restore_check.sh \
  /opt/open-public/backups/open_public-20261102T030405Z.dump
```

It creates a throwaway database named `restore_check_<epoch>` in the same
container, restores the dump into it with `pg_restore --no-owner
--no-privileges`, and prints one line per table in the public schema: the
table name and its row count. The last line reports the total and that the
check passed. The throwaway database is dropped on any exit, including
failure.

The check fails (non-zero exit) if `pg_restore` reports any error, or if no
table in the restored database has a single row. The second case is the point:
a dump that restores cleanly but lands empty is exactly what a silently failed
backup produces, and every command in such a run would otherwise have
succeeded.

Run the check against every backup on a schedule, or at least monthly. A
backup that has never been restored is not a backup.

## Restoring for real

A custom-format dump is restored with `pg_restore`, not `psql`. Create an
empty database first, then restore into it from inside the container:

```bash
docker exec open-public-postgres psql -U open_public -d postgres \
  -c 'CREATE DATABASE open_public_restore'
docker exec -i open-public-postgres pg_restore -U open_public --no-owner \
  -d open_public_restore \
  < /opt/open-public/backups/open_public-20261102T030405Z.dump
```

Restore into a new database first, never directly over a live one. Verify the
restored copy (row counts, application behavior), then switch the application
to it. `--no-owner` drops the original ownership records, which would
otherwise fail to apply to roles that do not exist in the target cluster.
