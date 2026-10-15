#!/bin/sh
# Applies supabase/migrations (mounted at /migrations) in filename order.
# Executed by the postgres image's entrypoint on first start only.
set -eu

for migration in /migrations/*.sql; do
  echo "applying $(basename "$migration")"
  psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" -f "$migration"
done
