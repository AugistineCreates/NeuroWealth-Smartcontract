# Local Development Stack

Run the whole off-chain app (Postgres, AI agent, WhatsApp bot and frontend)
with one command:

```bash
docker compose up
```

The root [`docker-compose.yml`](../docker-compose.yml) builds and starts:

| Service | URL | What it runs |
|---|---|---|
| `postgres` | `localhost:5432` (db `neurowealth`, user/pass `postgres`) | Postgres 16 with every `supabase/migrations/*.sql` applied |
| `agent` | <http://localhost:3001/health> | `agent/` from its Dockerfile, `AGENT_EXECUTION_MODE=dry-run` |
| `whatsapp` | <http://localhost:3010/health> | `whatsapp/` with signature validation skipped |
| `frontend` | <http://localhost:3000> | `frontend/` in `next dev` with hot reload |

The smart contract is not part of the stack. It is built and deployed with the
Stellar CLI (see the [README](../README.md#getting-started)); point the stack
at a deployed vault with `VAULT_CONTRACT_ID`.

## Prerequisites

- Docker with Compose v2 (`docker compose version`).
- Free ports 3000, 3001, 3010 and 5432, or override them (see below).

## Configuration

Every value has a working local default, so nothing is required for a first
run. Put overrides in a `.env` file next to `docker-compose.yml` (Compose reads
it automatically; it is git-ignored) or export them in your shell:

| Variable | Default | Used by | Effect |
|---|---|---|---|
| `VAULT_CONTRACT_ID` | empty (agent), placeholder (whatsapp) | all | Vault to watch and query. Empty disables the agent's event listener. |
| `SOROBAN_RPC_URL` | `https://soroban-testnet.stellar.org` | all | Soroban RPC endpoint. |
| `SOROBAN_NETWORK_PASSPHRASE` | testnet passphrase | agent, frontend | Network the RPC serves. |
| `OPENAI_API_KEYS` | empty | agent | Comma-separated keys for yield evaluation. |
| `NEXT_PUBLIC_SUPABASE_URL` / `NEXT_PUBLIC_SUPABASE_ANON_KEY` | empty | frontend | Needed for sign-in and realtime features. |
| `POSTGRES_PORT`, `AGENT_PORT`, `WHATSAPP_PORT`, `FRONTEND_PORT` | `5432`, `3001`, `3010`, `3000` | | Host ports. |
| `LOG_LEVEL` | `info` | agent | Pino log level. |

Example `.env` that watches a vault you deployed with `scripts/deploy-devnet.sh`:

```bash
VAULT_CONTRACT_ID=C...        # from scripts/devnet-contracts.env
OPENAI_API_KEYS=sk-...
```

## Common tasks

```bash
docker compose up -d                 # start in the background
docker compose logs -f agent         # follow one service
docker compose up postgres           # only the database (run services on the host)
docker compose restart whatsapp      # pick up code changes in whatsapp/
docker compose down                  # stop (keeps the database volume)
docker compose down -v               # stop and wipe the database and node_modules volumes
```

Connect to the database:

```bash
psql postgres://postgres:postgres@localhost:5432/neurowealth
```

Send a WhatsApp message without Twilio (signature checks are skipped locally):

```bash
curl -s localhost:3010/api/whatsapp/webhook \
  --data-urlencode 'From=whatsapp:+15550000001' \
  --data-urlencode 'Body=hi' \
  --data-urlencode 'MessageSid=SM00000000000000000000000000000001'
```

Repeat the same `MessageSid` to see a redelivery acknowledged with an empty
`<Response/>` (see [WHATSAPP_BOT.md](WHATSAPP_BOT.md#idempotent-webhook-processing)).

## How the database is initialised

On the first start with an empty `postgres_data` volume, the Postgres image runs
[`scripts/dev-stack/postgres-init/`](../scripts/dev-stack/postgres-init):

1. `00-supabase-shim.sql` creates what the migrations expect from Supabase:
   the `anon` / `authenticated` / `service_role` roles, the `auth.uid()`,
   `auth.role()` and `auth.jwt()` helpers, and the `supabase_realtime`
   publication. It is for local development only.
2. `10-apply-migrations.sh` applies `supabase/migrations/*.sql` in filename order
   and stops on the first error.

Services connect as `postgres`, the table owner, so row-level security policies
do not apply to them, the same as with the service role on Supabase.

Init scripts only run against an empty volume. After adding a migration, either
apply it by hand (`psql ... -f supabase/migrations/<file>.sql`) or reset with
`docker compose down -v && docker compose up`.

## Running a service on the host instead

Start only the database with `docker compose up postgres`, then run the service
you are working on directly:

```bash
# agent
cd agent && npm ci
DATABASE_URL=postgres://postgres:postgres@localhost:5432/neurowealth \
AGENT_EXECUTION_MODE=dry-run npx ts-node src/index.ts

# whatsapp
cd whatsapp && npm ci && cp .env.example .env   # fill in ENCRYPTION_KEY, PHONE_HASH_SALT, VAULT_CONTRACT_ID
DATABASE_URL=postgres://postgres:postgres@localhost:5432/neurowealth npx ts-node --transpile-only src/index.ts

# frontend
cd frontend && npm ci && npm run dev
```

## Known limitations

- **The frontend's auth and realtime need Supabase.** The stack's Postgres has
  no GoTrue or Realtime. For those features, point
  `NEXT_PUBLIC_SUPABASE_URL`/`NEXT_PUBLIC_SUPABASE_ANON_KEY` at a hosted
  project, or run `supabase start` and apply `supabase/migrations`.
- **The WhatsApp service runs with `ts-node --transpile-only`** because
  `whatsapp/` currently has type errors (`vaultRouter.ts` calls an RPC method
  missing from the installed SDK). Typecheck it with `npx tsc --noEmit`.
- **The agent runs in `dry-run`.** It builds and logs rebalance and compound
  transactions without submitting them. See
  [AGENT_DEPLOYMENT.md](AGENT_DEPLOYMENT.md) for live mode.
- `packages/monitoring/docker-compose.yml` (Prometheus/Grafana) is separate and
  not started by this file.
