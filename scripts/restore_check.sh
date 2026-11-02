#!/usr/bin/env bash
# Restore-test a dump produced by backup_db.sh: restore it into a throwaway
# database in the same container and report the row count of every table in
# the public schema. A backup is only worth trusting once it has actually been
# restored, and a dump that restores but lands empty is the failure this
# catches. The throwaway database is dropped on any exit, including failure.
#
#   CONTAINER  docker container running Postgres (required)
#   DB_USER    database role to connect as (default: open_public)
#
# usage: restore_check.sh <dump-file>
set -euo pipefail

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }
die() { log "error: $*"; exit 1; }

if [ $# -ne 1 ] || [ ! -f "$1" ]; then
  echo "usage: restore_check.sh <dump-file>" >&2
  exit 1
fi
DUMP="$1"

if [ -z "${CONTAINER:-}" ]; then
  echo "CONTAINER is required" >&2
  echo "usage: restore_check.sh <dump-file>" >&2
  exit 1
fi
DB_USER="${DB_USER:-open_public}"

# Set only once the database exists, so the trap never tries to drop a
# database that was never created. The name is generated, so it can never
# collide with a real database or need quoting in SQL.
THROWAWAY=""
drop_throwaway() {
  if [ -n "$THROWAWAY" ]; then
    if docker exec "$CONTAINER" psql -q -U "$DB_USER" -d postgres \
      -c "DROP DATABASE IF EXISTS $THROWAWAY" >/dev/null 2>&1; then
      log "dropped throwaway database $THROWAWAY"
    else
      log "warning: could not drop throwaway database $THROWAWAY"
    fi
  fi
}
trap drop_throwaway EXIT

NAME="restore_check_$(date -u +%s)"
log "creating throwaway database $NAME in container $CONTAINER"
docker exec "$CONTAINER" psql -q -U "$DB_USER" -d postgres -c "CREATE DATABASE $NAME"
THROWAWAY="$NAME"

# pg_restore exits non-zero when any statement in the restore failed, so
# set -e turns a broken dump into a failed check. --no-owner and
# --no-privileges because a throwaway has no one to grant the original
# ownership to.
log "restoring $DUMP into $THROWAWAY"
if ! docker exec -i "$CONTAINER" pg_restore -U "$DB_USER" --no-owner \
  --no-privileges -d "$THROWAWAY" < "$DUMP"; then
  die "restore reported errors"
fi

# quote_ident in the listing so a name needing quotes survives the count
# query unchanged. One psql per table keeps each count a real count(*).
log "counting rows in the public schema of $THROWAWAY"
tables="$(docker exec "$CONTAINER" psql -q -At -U "$DB_USER" -d "$THROWAWAY" -c "
  SELECT quote_ident(table_name)
  FROM information_schema.tables
  WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
  ORDER BY table_name")"

total=0
while IFS= read -r table; do
  [ -n "$table" ] || continue
  count="$(docker exec "$CONTAINER" psql -q -At -U "$DB_USER" -d "$THROWAWAY" \
    -c "SELECT count(*) FROM $table")"
  printf '%s %s\n' "$table" "$count"
  total=$((total + count))
done <<< "$tables"

# An empty dataset is exactly what a silently-failed dump produces, so zero
# rows everywhere fails the check even though every command succeeded.
if [ "$total" -eq 0 ]; then
  die "no table in the restored database has any row"
fi
log "restore check passed: $total rows in the public schema"
