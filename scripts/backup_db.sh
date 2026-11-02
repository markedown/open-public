#!/usr/bin/env bash
# Back up the project database with pg_dump running inside the Postgres
# container, then prune old dumps and optionally ship the new one off-box.
#
# The container's own pg_dump is used, never a host client: client and server
# majors can differ, and a mismatched pg_dump can produce an empty archive
# that looks like a successful backup right up to the moment it is needed.
#
# Configuration is environment-only so the script can run unmodified from
# cron or a deployment hook:
#   CONTAINER   docker container running Postgres (required)
#   BACKUP_DIR  directory the dumps are written to (required)
#   DB_USER     database role to dump as (default: open_public)
#   DB_NAME     database to dump (default: open_public)
#   KEEP_DAYS   days of dump history to retain (default: 14)
#   UPLOAD_CMD  optional shell command, run as: $UPLOAD_CMD <dump-path>
set -euo pipefail

usage() {
  cat <<'EOF'
usage: set CONTAINER and BACKUP_DIR, then run backup_db.sh

  CONTAINER   docker container running Postgres (required)
  BACKUP_DIR  directory the dumps are written to (required)
  DB_USER     database role to dump as (default: open_public)
  DB_NAME     database to dump (default: open_public)
  KEEP_DAYS   days of dump history to retain (default: 14)
  UPLOAD_CMD  optional shell command, run as: $UPLOAD_CMD <dump-path>
EOF
}

log() { printf '%s %s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$*"; }
die() { log "error: $*"; exit 1; }

missing=""
[ -n "${CONTAINER:-}" ] || missing="$missing CONTAINER"
[ -n "${BACKUP_DIR:-}" ] || missing="$missing BACKUP_DIR"
if [ -n "$missing" ]; then
  echo "missing required environment variables:$missing" >&2
  usage >&2
  exit 1
fi

DB_USER="${DB_USER:-open_public}"
DB_NAME="${DB_NAME:-open_public}"
KEEP_DAYS="${KEEP_DAYS:-14}"

# KEEP_DAYS reaches find as a numeric argument, so reject anything else rather
# than let a typo turn into a wrong deletion window.
case "$KEEP_DAYS" in
  '' | *[!0-9]*) die "KEEP_DAYS must be a non-negative integer, got: $KEEP_DAYS" ;;
esac

# A dump holds every user row, including password hashes, so only the owner may
# read it, whatever the caller's umask is.
umask 077
mkdir -p "$BACKUP_DIR"

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
FINAL="$BACKUP_DIR/$DB_NAME-$STAMP.dump"
PARTIAL="$FINAL.partial"

# A dump is only ever renamed once it verifies; the trap discards the partial
# file on any failure path. Clearing PARTIAL after the rename keeps a
# successful run's dump out of the trap's way.
trap 'if [ -n "$PARTIAL" ]; then rm -f "$PARTIAL"; fi' EXIT

log "dumping database $DB_NAME from container $CONTAINER"
docker exec "$CONTAINER" pg_dump -U "$DB_USER" -d "$DB_NAME" -Fc > "$PARTIAL"

# Verify before renaming: a truncated or version-mismatched archive must not
# enter the retention pool. Size first (cheap), then the container's
# pg_restore reading the archive from stdin.
size="$(( $(wc -c < "$PARTIAL") ))"
if [ "$size" -lt 1024 ]; then
  die "dump is only $size bytes, refusing to keep it"
fi
if ! toc="$(docker exec -i "$CONTAINER" pg_restore --list < "$PARTIAL")"; then
  die "pg_restore could not read the dump, refusing to keep it"
fi
if [ -z "$toc" ]; then
  die "dump lists no contents, refusing to keep it"
fi

mv "$PARTIAL" "$FINAL"
PARTIAL=""
log "wrote $FINAL ($size bytes)"

# -mtime +N and -exec rm are supported by both BSD and GNU find; the partial
# file pattern never matches, so an aborted run is not pruned as if complete.
log "deleting $DB_NAME dumps in $BACKUP_DIR older than $KEEP_DAYS days"
find "$BACKUP_DIR" -maxdepth 1 -type f -name "$DB_NAME-*.dump" \
  -mtime "+$KEEP_DAYS" -exec rm -f {} +

if [ -n "${UPLOAD_CMD:-}" ]; then
  log "uploading $FINAL"
  # sh -c so UPLOAD_CMD may be a multi-word command; the dump path is passed
  # as $1 inside the command so paths with spaces survive. A failed upload
  # exits non-zero but leaves the verified local dump untouched.
  if ! sh -c "$UPLOAD_CMD \"\$1\"" sh "$FINAL"; then
    die "upload failed, local dump kept at $FINAL"
  fi
  log "upload finished"
fi

log "backup complete"
