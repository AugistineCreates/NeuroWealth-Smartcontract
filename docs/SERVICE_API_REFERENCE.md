# Off-chain Service API Reference

HTTP endpoints exposed by the off-chain services. For the on-chain vault
interface, see [EVENTS.md](../EVENTS.md), [ARCHITECTURE.md](../ARCHITECTURE.md)
and `contract-spec.json`.

| Service | Default port | Source |
|---|---|---|
| AI agent | `3001` (`PORT`) | `agent/src/index.ts`, `agent/src/health.ts` |
| Agent metrics server | `9100` (`METRICS_PORT`) | `agent/src/metricsApi.ts` |
| WhatsApp bot | `3000` (`PORT`), `3010` in the local stack | `whatsapp/src/index.ts` |

---

## AI agent

All routes go through two rate limiters. Defaults: 30 requests per IP and
20 per user, per 60 s window (`RATE_LIMIT_WINDOW_MS`, `RATE_LIMIT_MAX_REQUESTS`,
`USER_RATE_LIMIT_MAX_REQUESTS`). The user key comes from the `X-User-ID` (or
`user-id`) header, falling back to the IP. When a limit is exceeded:

```http
HTTP/1.1 429 Too Many Requests
Retry-After: 60

{ "error": "Too many requests from this IP, please try again later.", "retryAfterSeconds": 60 }
```

### `GET /health`

Deep health check: runs `SELECT 1`, calls `getLatestLedger` on the RPC, and
checks OpenAI key health.

| Status | When |
|---|---|
| `200` | Every check is `ok` or `not_configured` |
| `503` | Any check is `error` (DB, RPC) or `degraded` (every OpenAI key unhealthy) |

```json
{
  "status": "ok",
  "checks": { "database": "ok", "stellar_rpc": "ok", "openai_api_keys": "not_configured" },
  "metrics": {
    "dbPool": { "...": "pool counters from getPoolMetrics()" },
    "openAiKeys": { "totalConfigured": 0, "healthyCount": 0, "keys": [] }
  },
  "uptime": 123.45,
  "timestamp": "2026-10-15T12:00:00.000Z"
}
```

Use it for liveness only with a generous failure threshold, since it fails
during upstream outages that a restart does not fix.

### `GET /ready`

Readiness: `200 { "ready": true }` when the Soroban RPC answers
`getLatestLedger`, otherwise `503 { "ready": false, "error": "Stellar RPC unreachable" }`.

---

## Agent metrics server

`startMetricsServer()` serves the `MetricsEngine` snapshot on `METRICS_PORT`.
**It is not started by `agent/src/index.ts` today.** Wire it in before
depending on these endpoints in a deployment.

| Route | Response |
|---|---|
| `GET /health` | `200`/`503` `{ status: "healthy" \| "degraded", agentHealthy, lastHeartbeatAt, uptimePct }` (used by the failover monitor, [DISASTER_RECOVERY.md](DISASTER_RECOVERY.md)) |
| `GET /api/metrics/snapshot` | Full `MetricsSnapshot` for the dashboard |
| `GET /api/metrics/alerts` | `{ generatedAt, alerts: MetricAlert[] }` for currently firing threshold alerts |
| `GET /api/metrics/database-retries` | Retry counters for transient PostgreSQL failures |

See `agent/src/metrics.ts` for the payload types and alert thresholds.

---

## WhatsApp bot

Both Twilio routes accept `application/x-www-form-urlencoded` (Twilio's
format) and JSON. Both require a valid `X-Twilio-Signature`; validation can be
skipped only outside production with `TWILIO_SKIP_SIGNATURE_VALIDATION=true`.

| Status | Signature failure |
|---|---|
| `500` | `TWILIO_AUTH_TOKEN` not configured |
| `403` | Missing or invalid signature, or plain HTTP in production |

### `GET /health`

`200 { "status": "ok", "service": "NeuroWealth WhatsApp Bot Handler" }`.

### `POST /api/whatsapp/webhook`

The inbound message webhook, configured in the Twilio console. It runs the
conversation state machine (see [WHATSAPP_BOT.md](WHATSAPP_BOT.md)) and replies
with TwiML.

| Field | Required | Notes |
|---|---|---|
| `From` | yes | `whatsapp:+E164`. Hashed with `PHONE_HASH_SALT` and never stored in the clear. |
| `Body` | no | Message text. |
| `MessageSid` | expected | Idempotency key, scoped by `AccountSid`. If it is absent, the `I-Twilio-Idempotency-Token` header is used. With neither, the message is processed without deduplication. |
| `AccountSid` | expected | Scopes the idempotency key. |

| Status | Body | Meaning |
|---|---|---|
| `200` | `<Response><Message>…</Message></Response>` | Command processed; the message is the reply. |
| `200` | `<Response></Response>` | Duplicate of a delivery that already completed (or was abandoned mid-side-effect). Nothing re-run, no second reply. |
| `400` | text | `From` missing. |
| `409` | text | The same delivery is being processed by another worker. Safe to retry. |
| `503` | text | Retryable failure before any side effect (for example, the DB is unreachable). Redelivery resumes it. |

Per-phone rate limit: 10 messages per 60 s (`WHATSAPP_RATE_LIMIT_*`). Over the
limit, the reply is a TwiML warning with status `200`.

### `POST /api/whatsapp/status`

Twilio delivery-status callback for outbound messages. The signed URL must
match `TWILIO_STATUS_CALLBACK_URL`.

| Field | Notes |
|---|---|
| `MessageSid` (or `SmsSid`) | Key in `whatsapp_message_deliveries`. |
| `MessageStatus` | `queued` → `sent` → `delivered` \| `failed` \| `undelivered`. Applied forward-only, so duplicates and out-of-order callbacks are no-ops. |
| `To` | Stored only encrypted. |
| `ErrorCode`, `ErrorMessage` | Phone-number-like substrings are redacted before storage. |

It always responds with an empty `200`, even for unknown statuses or
persistence errors, so Twilio does not retry-storm the endpoint.

---

## Frontend

The Next.js app (`frontend/`) has no server API routes. It reads chain state
directly from Soroban RPC and user data from Supabase (`NEXT_PUBLIC_*`
variables). See [frontend/README.md](../frontend/README.md).
