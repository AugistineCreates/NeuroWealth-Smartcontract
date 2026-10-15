# WhatsApp Bot Architecture

> **Scope:** `whatsapp/` (`neurowealth-whatsapp`), the Twilio webhook service that
> lets users use the vault over WhatsApp chat (Issues #469, #767, #771).

## Overview

The bot is a small Express service. Twilio forwards each inbound WhatsApp
message to a webhook. The bot runs a per-phone state machine (verify with OTP,
then chat), parses the message into an intent, validates it against the vault
contract's limits, and replies with TwiML in the same HTTP response. Users get a
**custodial** Stellar keypair, which the bot encrypts at rest.

```
 WhatsApp user
      │  message
      ▼
   Twilio  ── POST /api/whatsapp/webhook (form-encoded: From, Body) ──►  Express (index.ts)
                                                                          │
                                                                          ▼
                                                                   webhook.ts
      ┌───────────────────────────────────────────────────────────────────┤
      │ 1. hashPhoneNumber(From)            cryptoUtils.ts (SHA-256 + salt)│
      │ 2. checkRateLimit(phoneHash)        stateManager.ts (10 msg / min) │
      │ 3. getSession(phoneHash)            stateManager.ts (15 min idle)  │
      │ 4. parseIntent(Body)                intentParser.ts                │
      │ 5. state machine:                                                  │
      │      UNVERIFIED   → generateOTP     otpService.ts                  │
      │      AWAITING_OTP → verifyOTP → createCustodialWallet              │
      │                                     walletService.ts (AES-256-GCM) │
      │      VERIFIED     → validateIntent  contractLimits.ts              │
      │                   → getPortfolio / handleDeposit / handleWithdraw  │
      │                                     vaultRouter.ts ─► Soroban RPC  │
      └───────────────────────────────────────────────────────────────────┤
                                                                          ▼
                                                        TwiML <Message> response ──► Twilio ──► user
```

## Modules

| File | Responsibility |
|---|---|
| `src/index.ts` | Express bootstrap, `dotenv`, body parsers, `GET /health`, `POST /api/whatsapp/webhook`. |
| `src/webhook.ts` | Twilio handler and conversation state machine; builds TwiML replies. |
| `src/webhookIdempotency.ts` | Idempotency key derivation and the `whatsapp_inbound_webhooks` processing ledger that makes redelivered webhooks run at most once. |
| `src/stateManager.ts` | In-memory sessions (`UNVERIFIED` → `AWAITING_OTP` → `VERIFIED`), 15-minute inactivity reset, fixed-window rate limit (10 messages / 60 s per phone). |
| `src/otpService.ts` | 6-digit OTP from `crypto.randomInt`, 5-minute TTL, max 3 attempts. |
| `src/walletService.ts` | Creates a custodial `Keypair` per verified phone; stores only the public key and the encrypted secret. |
| `src/cryptoUtils.ts` | Salted SHA-256 phone hashing; AES-256-GCM encrypt/decrypt of secret keys (key derived with `scrypt` from `ENCRYPTION_KEY`). |
| `src/intentParser.ts` | Keyword/regex parser → `GREETING`, `OTP_CODE`, `DEPOSIT`, `WITHDRAW`, `BALANCE`, `EARNINGS`, `STRATEGY`, `APY`, `UNKNOWN`. |
| `src/contractLimits.ts` | Mirrors the vault's deposit caps and strategy names; `validateIntent` rejects out-of-range amounts and unknown strategies before any transaction is built (#771). |
| `src/vaultRouter.ts` | Portfolio reads and deposit/withdraw calls against the vault contract. |

## Conversation flow

1. **UNVERIFIED:** any greeting or unrecognised message creates an OTP and
   moves the session to `AWAITING_OTP`.
2. **AWAITING_OTP:** a 6-digit code is checked by `verifyOTP`. On success the
   session becomes `VERIFIED` and a custodial wallet is created. Expired codes
   or three wrong attempts discard the OTP; the user must send `hi` again.
3. **VERIFIED:** each intent is first checked by `validateIntent`, then routed:
   - `BALANCE` / `EARNINGS` / `APY` → `getPortfolio`
   - `DEPOSIT` → `handleDeposit(amount, strategy?)`
   - `WITHDRAW` → `handleWithdraw(amount | all)`
   - `STRATEGY` → strategy confirmation (applied by the agent on its next run)
4. After 15 minutes without a message the session drops back to `UNVERIFIED`.

## Idempotent webhook processing

Twilio can redeliver the same inbound webhook after a timeout. Each delivery
is keyed by `twilio:inbound-message:<AccountSid>:<MessageSid>` (falling back to
the `I-Twilio-Idempotency-Token` header), and a row in
`whatsapp_inbound_webhooks` is claimed with an atomic upsert **before** the
command runs, inside the per-phone command lock.

| Delivery sees | Response | Command re-run? |
|---|---|---|
| new key | normal TwiML reply | runs once |
| `completed` | `200`, empty `<Response/>` (no second reply) | no |
| `processing`, lease live (another worker) | `409` | no, a later redelivery sees the result |
| `failed` (retryable, before side effects) | resumes as a new attempt | yes, safe |
| `processing`, lease expired, no side effects | resumes as a new attempt | yes, safe |
| `processing`, lease expired, side effects started | `200`, empty; row → `abandoned` | no, needs manual reconciliation |

The handler calls `SideEffectGuard.begin()` (persisting `side_effects_started`)
immediately before any state-changing step: OTP issue/verify, wallet creation,
deposit, withdraw, strategy change. A transient error (connection refused,
timeout, PostgreSQL class `08`, serialization failure, ...) raised before that
point marks the row `failed` and returns `503` so the next delivery retries.
Any error after it becomes the usual user-facing error reply and the row is
`completed`, because the command may already have partially applied.

`attempt_count` is a fencing token: a worker whose lease was taken over cannot
mark side effects or complete the newer attempt. Reply text is never stored
(it can contain OTP codes), so a duplicate is acknowledged without replaying
the reply. Webhooks with neither a MessageSid nor an idempotency token are
processed without deduplication and logged as a warning.

## Intent validation against contract caps (#771)

`validateIntent` (in `src/contractLimits.ts`) runs before any vault call:

| Intent | Rule | Contract source |
|---|---|---|
| `DEPOSIT` | amount required, `> 0`, at most 7 decimals (USDC stroop precision) | USDC has 7 decimals |
| `DEPOSIT` | `>= min deposit` (default 0.1 USDC) | `MinDeposit` / `get_min_deposit` |
| `DEPOSIT` | `<= max deposit` (default 1,000 USDC) | `MaxDeposit` / `get_max_deposit` |
| `DEPOSIT` | `<= per-user cap` (default 1,000 USDC) | `UserDepositCap` / `get_user_deposit_cap` |
| `DEPOSIT` | `<= TVL cap` (default 10,000 USDC) | `TvLCap` / `get_tvl_cap` |
| `DEPOSIT`, `STRATEGY` | strategy ∈ `conservative`, `balanced`, `growth` | `set_user_strategy` → `InvalidStrategy` |
| `WITHDRAW` | `withdraw all`, or an amount `> 0` with at most 7 decimals | none |

The defaults match the contract's `initialize` values in
`neurowealth-vault/contracts/vault/src/lib.rs`. If the owner changes limits
on-chain, set the `VAULT_*` env vars below to the values returned by the
contract getters. Note that deposits no longer default to 100 USDC when the
user leaves out the amount: the bot asks for one instead.

The per-user and TVL checks look at the requested amount only. The contract
still enforces them against the user's and the vault's existing totals, so this
check catches obviously invalid requests early but does not replace on-chain
enforcement.

## Environment variables

Copy `whatsapp/.env.example` to `whatsapp/.env` (git-ignored). `dotenv` loads it
at startup.

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `PORT` | no | `3000` | HTTP port for Express. |
| `ENCRYPTION_KEY` | **yes in any shared or prod environment** | insecure built-in string | Secret used to derive the AES-256-GCM key for custodial secrets. Use ≥ 32 random bytes. Rotating it makes existing encrypted secrets undecryptable. |
| `PHONE_HASH_SALT` | **yes in any shared or prod environment** | insecure built-in string | Salt for SHA-256 phone hashing. Changing it orphans every existing session and wallet. |
| `TWILIO_AUTH_TOKEN` | **yes** | — | Verifies the `X-Twilio-Signature` header on every webhook request (`src/twilioSignature.ts`). If unset, every webhook request is rejected with `500`. |
| `TWILIO_WEBHOOK_URL` | behind a proxy/tunnel | reconstructed from the request | Public webhook URL exactly as configured in the Twilio console. Twilio signs this URL, so it must match byte for byte. |
| `TWILIO_SKIP_SIGNATURE_VALIDATION` | no | `false` | `true` disables signature checks for local testing with curl. Ignored when `NODE_ENV=production`. |
| `SOROBAN_RPC_URL` | no | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint for vault calls. |
| `VAULT_CONTRACT_ID` | yes outside testnet demos | a testnet contract id | Vault contract address. |
| `VAULT_MIN_DEPOSIT` | no | `1000000` (0.1 USDC) | Min single deposit, **in stroops**. Mirror `get_min_deposit`. |
| `VAULT_MAX_DEPOSIT` | no | `10000000000` (1,000 USDC) | Max single deposit, in stroops. Mirror `get_max_deposit`. |
| `VAULT_USER_DEPOSIT_CAP` | no | `10000000000` (1,000 USDC) | Per-user cap, in stroops. Mirror `get_user_deposit_cap`. |
| `VAULT_TVL_CAP` | no | `100000000000` (10,000 USDC) | Vault TVL cap, in stroops. Mirror `get_tvl_cap`. |
| `DATABASE_URL` | yes | `postgresql://postgres:postgres@localhost:5432/neurowealth` | Postgres holding `whatsapp_wallets`, `whatsapp_message_deliveries` and `whatsapp_inbound_webhooks`. |
| `WHATSAPP_WEBHOOK_LEASE_MS` | no | `120000` | How long a processing attempt holds an inbound webhook before a redelivery may take it over. Keep it well above the slowest command. |

Invalid `VAULT_*` values (non-integer, zero or negative) make `validateIntent`
throw, so the webhook replies with the generic error message rather than
silently using a wrong limit.

Replies are sent as TwiML in the webhook response, so only `TWILIO_AUTH_TOKEN`
is needed today. `TWILIO_ACCOUNT_SID` and `TWILIO_WHATSAPP_NUMBER` become
required once outbound (non-reply) messages are added.

Requests without a valid `X-Twilio-Signature` are rejected with `403` before
they reach the state machine, so a caller cannot impersonate a phone number by
posting a forged `From`.

## Running locally

```bash
cd whatsapp
cp .env.example .env      # then fill in ENCRYPTION_KEY, PHONE_HASH_SALT and TWILIO_AUTH_TOKEN
npm install
npm run dev               # ts-node src/index.ts
npm test                  # tsc + node:test (intent validation tests)
ngrok http 3000           # expose the webhook
```

In the Twilio console (Messaging → WhatsApp sandbox or sender), set **"When a
message comes in"** to `https://<ngrok-host>/api/whatsapp/webhook`, method
`POST`, and set `TWILIO_WEBHOOK_URL` to that same URL. `GET /health` returns `{"status":"ok"}` for liveness checks.

## Security notes and known gaps

These gaps are in the current implementation. Close them before the bot handles
real funds:

- **In-memory state.** Sessions, OTPs and custodial wallets live in process
  `Map`s. A restart loses every wallet, including the only copy of its encrypted
  secret. Persist wallets to the database (`db/` / `supabase/`) first.
- **OTP is echoed in chat.** The welcome message includes the OTP for demo
  purposes, so it does not prove ownership of the phone. Deliver the OTP over a
  separate channel (SMS / Twilio Verify) and remove it from the reply.
- **Insecure defaults.** `ENCRYPTION_KEY` and `PHONE_HASH_SALT` fall back to
  hard-coded strings, and `scrypt` uses a fixed salt. Fail fast at startup when
  they are unset in production.
- **Vault calls are simulated.** `vaultRouter.ts` returns fixed portfolio values
  and fake transaction hashes. Real integration should use
  `@neurowealth/vault-client` and sign with the decrypted custodial key. The
  key must never be logged or sent in chat.
- **Single-process rate limiting.** The limiter is per instance. Running more
  than one replica needs a shared store (e.g. Redis).

See also [SECRETS_HYGIENE.md](SECRETS_HYGIENE.md) and the
[WhatsApp Integration](../README.md#whatsapp-integration) section of the README.
