# Agent Deployment Guide

How to run the NeuroWealth AI agent (`agent/`) in a shared environment
(testnet or mainnet). For a laptop setup, see
[LOCAL_DEV_STACK.md](LOCAL_DEV_STACK.md). For module internals, see
[agent/README.md](../agent/README.md).

## What the agent does at runtime

- **Event listener.** Polls the vault's Soroban events (deposit and withdraw),
  logs them to `vault_events`, runs alert rules, and on deposits evaluates yield
  and may submit a rebalance. It persists its position in `agent_state`.
- **Hourly decision loop.** Loads every user's strategy, evaluates yield, then
  either rebalances or calls `auto_compound`.
- **Nightly rollup.** Aggregates and cleans up analytics tables when
  `DATABASE_URL` is set.
- **HTTP API.** Serves `/health` and `/ready` on `PORT` (default `3001`). See
  [SERVICE_API_REFERENCE.md](SERVICE_API_REFERENCE.md).

Run **exactly one** agent instance per vault. The listener cursor and the
decision loop are not coordinated across replicas, so two instances would
handle each event twice and race on rebalances. Use a single replica with
restart-on-failure, not horizontal scaling.

## Pre-deployment checklist

1. The vault is deployed and initialised, and its contract id is known. For
   mainnet, complete [MAINNET_CHECKLIST.md](MAINNET_CHECKLIST.md) first.
2. The agent account is registered as the vault's agent. Follow the separate
   rebalancer/reporter roles in
   [LEAST_PRIVILEGE_AGENT.md](LEAST_PRIVILEGE_AGENT.md).
3. A Postgres database (Supabase or managed Postgres) has every
   `supabase/migrations/*.sql` applied, in filename order.
4. Secrets are in a secret manager, not in the image or the repo. See
   [SECRETS_HYGIENE.md](SECRETS_HYGIENE.md).
5. The compromise runbook is understood:
   [AGENT_KEY_COMPROMISE_RUNBOOK.md](AGENT_KEY_COMPROMISE_RUNBOOK.md).

## Configuration

### Required

| Variable | Notes |
|---|---|
| `AGENT_EXECUTION_MODE` | `dry-run` or `live`; there is no default, and the agent refuses to start without it. Legacy `DRY_RUN`/`LIVE_MODE` are rejected. |
| `DATABASE_URL` | Postgres connection string. Without it, strategies, rollups and DB logging are disabled and the cursor falls back to a local file. Do not run that way in a shared environment. |
| `VAULT_CONTRACT_ID` | Vault to monitor. Without it, the event listener does not start. |
| `SOROBAN_RPC_URL` | Soroban RPC endpoint. Use a provider with enough event retention to cover your longest expected outage (see [Cursor and restarts](#cursor-and-restarts)). |
| `SOROBAN_NETWORK_PASSPHRASE` | Must match the RPC's network. |
| `SOROBAN_SECRET_KEY` | Signing key for `live` mode. In `dry-run`, `SOROBAN_PUBLIC_KEY` is enough for simulation. |

### Common optional settings

| Variable | Default | Purpose |
|---|---|---|
| `PORT` | `3001` | HTTP API port. |
| `OPENAI_API_KEYS` | none | Comma-separated keys, rotated on failure (`OPENAI_API_KEY` for a single key). |
| `START_LEDGER` | latest ledger | First-run start position, used only when no cursor is stored. |
| `SOROBAN_POLL_INTERVAL_MS` | `5000` | Base event-poll interval. |
| `SOROBAN_POLL_MAX_BACKOFF_MS` | `60000` | Backoff cap while RPC or cursor writes fail. |
| `SOROBAN_CIRCUIT_BREAKER_THRESHOLD` / `_COOLDOWN_MS` | `5` / `30000` | Consecutive failures before the breaker opens, and its cooldown. |
| `MAX_DB_POOL_SIZE` / `MIN_DB_POOL_SIZE` | see `agent/src/db.ts` | Postgres pool sizing. |
| `DB_RETRY_*`, `DB_TRANSACTION_DEADLINE_MS` | see agent README | Retry budget for transient DB errors. |
| `RATE_LIMIT_WINDOW_MS`, `RATE_LIMIT_MAX_REQUESTS`, `USER_RATE_LIMIT_MAX_REQUESTS` | | HTTP rate limits. |
| `JAEGER_ENDPOINT` | none | OpenTelemetry trace export. |
| `LOG_LEVEL` | `info` | Pino log level. |

## Building and running

The image is built from [`agent/Dockerfile`](../agent/Dockerfile) and listens
on `3001`:

```bash
docker build -t neurowealth-agent ./agent
docker run -d --name neurowealth-agent \
  --init --stop-timeout 20 --restart unless-stopped \
  --env-file /secure/path/agent.env \
  -p 3001:3001 neurowealth-agent
```

- `--init` (or `init: true` in Compose) makes sure `SIGTERM` reaches Node
  through `npx`.
- `--stop-timeout 20` (or `stop_grace_period: 20s`, or Kubernetes
  `terminationGracePeriodSeconds: 20`) must be longer than the agent's 10 s
  shutdown budget, or the runtime kills it mid-flush.
- On Kubernetes, use `/ready` as the readiness probe and `/health` as the
  liveness probe, with a generous failure threshold. `/health` returns 503
  when the DB, RPC or every OpenAI key is down, and a restart does not fix an
  upstream outage.

## Rollout procedure

1. Apply new migrations before deploying code that needs them.
2. Deploy in **`dry-run`** first. Watch the logs for `Resuming event listener
   from saved cursor` (or `Starting event listener from initial ledger` on the
   very first run), then for the hourly evaluation and simulated transactions.
3. Switch to `live` by changing `AGENT_EXECUTION_MODE` and restarting.
4. Because only one instance may run, upgrades are stop-then-start, not
   rolling. The graceful shutdown makes this safe: the old instance flushes its
   cursor before exiting, and the new one resumes with the next event.

## Cursor and restarts

The listener stores the RPC paging token and the last handled event id in
`agent_state` (key `agent:ledger_cursor`):

- **Clean restart** (SIGTERM). The in-flight event finishes and the cursor is
  flushed. Nothing is replayed or skipped.
- **Crash or `SIGKILL`.** At most the single event being handled at that
  moment can be replayed. Rebalances are idempotent by decision id, and
  `vault_events` inserts are `ON CONFLICT DO NOTHING`.
- **Cursor write failure.** Processing pauses with backoff until the write
  succeeds, so the stored cursor never lags more than one event.
- **Outage longer than RPC retention.** The listener logs `events in the gap
  were MISSED and need manual backfill` and resumes from the tip. Backfill
  from an archival RPC or Horizon, then reconcile `vault_events`.
- **Unreadable cursor at startup** (for example, the DB is down). The listener
  refuses to start rather than skip ahead. Fix the DB and restart.

To replay from a specific point, stop the agent, delete or edit the
`agent:ledger_cursor` row (or set `START_LEDGER` after deleting it), and start
again.

## Operations

- **Logs.** Structured JSON (pino) on stdout; secrets are redacted.
- **Metrics and alerts.** See [monitoring.md](monitoring.md) and
  [SECURITY_MONITORING.md](SECURITY_MONITORING.md).
  `packages/monitoring/` ships a Prometheus/Grafana Compose setup.
- **Incidents.** See [INCIDENT_RESPONSE.md](INCIDENT_RESPONSE.md),
  [DISASTER_RECOVERY.md](DISASTER_RECOVERY.md) and
  [REBALANCE_FAILURE_RECOVERY.md](REBALANCE_FAILURE_RECOVERY.md).
