# Architecture Decision Record Index

NeuroWealth's design decisions live in topic documents rather than numbered
ADR files. This index lists every one, says what it decides or covers, and
links to its tracking issue, so you can find the source of truth for any
design question without searching `docs/`.

**Kinds**

- **Decision**: records a choice and its alternatives (ADR-style).
- **Design**: specifies how a mechanism works.
- **Reference**: describes the current system or an interface.
- **Runbook**: a procedure for operators.
- **Policy**: rules contributors or operators follow.

The **Status** column shows the status the document declares, or "—" when it
declares none.

## Vault contract: core design

| Document | Kind | What it decides or covers | Issue | Status |
|---|---|---|---|---|
| [ARCHITECTURE.md](../ARCHITECTURE.md) | Reference | Storage layout, share-accounting math, asset flows | #228 | — |
| [state-machine.md](state-machine.md) | Reference | Contract lifecycle states and allowed transitions | | — |
| [EVENTS.md](../EVENTS.md) | Reference | Every emitted event: topics and payload schemas | #847 | — |
| [ERC4626_CONFORMANCE_CHECKLIST.md](ERC4626_CONFORMANCE_CHECKLIST.md) | Reference | Function-by-function deviations from ERC-4626 | #602 | — |
| [ERROR_STYLE_GUIDE.md](../ERROR_STYLE_GUIDE.md) | Policy | Error codes and message wording | | — |
| [WITHDRAWAL_QUEUE.md](WITHDRAWAL_QUEUE.md) | Design | Queued withdrawals when idle liquidity is short | | — |
| [PARTIAL_WITHDRAWAL_BEHAVIOR.md](PARTIAL_WITHDRAWAL_BEHAVIOR.md) | Decision | Withdrawal fairness under a liquidity crunch (replaces an earlier queue design) | #600 | — |
| [INSURANCE.md](INSURANCE.md) | Design | Insurance fund funded from yield, and payout on incidents | | — |
| [REWARDS.md](REWARDS.md) | Design | Claiming and compounding liquidity-mining rewards | | — |

## Vault contract: safety controls and governance

| Document | Kind | What it decides or covers | Issue | Status |
|---|---|---|---|---|
| [SECURITY.md](../SECURITY.md) | Reference | Trust and threat model, pause semantics, owner-compromise runbook | #563 | — |
| [CIRCUIT_BREAKER.md](CIRCUIT_BREAKER.md) | Design | Automatic halt on anomalous behaviour or market stress | | — |
| [ROLLING_DECREASE_CAP_DESIGN.md](ROLLING_DECREASE_CAP_DESIGN.md) | Decision | Rolling-window cap on `update_total_assets` decreases | #589 | Design Decision Recorded |
| [CAP_CHANGE_TIMELOCK_DESIGN.md](CAP_CHANGE_TIMELOCK_DESIGN.md) | Decision | Two-step confirmation for large admin cap changes | #590 | Design Decision Recorded |
| [GUARDIAN_KEY_DESIGN.md](GUARDIAN_KEY_DESIGN.md) | Decision | Second (guardian) signature for `execute_upgrade` | #607 | Design Decision Recorded |
| [LEAST_PRIVILEGE_AGENT.md](LEAST_PRIVILEGE_AGENT.md) | Decision | Separate rebalancer and reporter agent roles | #606 | Design Decision Recorded |
| [ISSUER_FREEZE_CONTINGENCY.md](ISSUER_FREEZE_CONTINGENCY.md) | Runbook | What to do if USDC or the agent wallet is frozen by the issuer | #604 | — |

## Vault contract: upgrades, verification and release

| Document | Kind | What it decides or covers | Issue | Status |
|---|---|---|---|---|
| [UPGRADE_MIGRATION.md](UPGRADE_MIGRATION.md) | Policy | Contract upgrades and storage-schema migration rules | #637 | — |
| [BALANCE_DEPRECATION_MIGRATION.md](BALANCE_DEPRECATION_MIGRATION.md) | Decision | Deprecating `DataKey::Balance` storage | | — |
| [DEPRECATION.md](../DEPRECATION.md) | Reference | Deprecated functions and their replacements | | — |
| [FORMAL_VERIFICATION.md](FORMAL_VERIFICATION.md) | Design | Kani proofs and share-accounting properties | #672 | Kani proofs in CI |
| [WASM_SIZE.md](WASM_SIZE.md) | Policy | 1.5 MB optimised WASM budget enforced in CI | | — |
| [E2E_ARTIFACT_LIFECYCLE.md](E2E_ARTIFACT_LIFECYCLE.md) | Reference | What `scripts/e2e-devnet.sh` produces and keeps | | — |
| [AUDIT_PREP.md](AUDIT_PREP.md) | Reference | External audit package | #611 | Pre-audit draft |
| [MAINNET_CHECKLIST.md](MAINNET_CHECKLIST.md) | Runbook | Mandatory pre-mainnet sign-offs | #316 | — |

## Yield strategy and protocol integrations

| Document | Kind | What it decides or covers | Issue | Status |
|---|---|---|---|---|
| [PROTOCOL_ADAPTER_INTERFACE.md](PROTOCOL_ADAPTER_INTERFACE.md) | Design | Adapter interface for multi-protocol yield aggregation | #656 | — |
| [BLEND_INTEGRATION_RESEARCH.md](BLEND_INTEGRATION_RESEARCH.md) | Decision | Blend pool supply and withdraw design, cross-contract calls | | — |
| [DEX_INTEGRATION.md](DEX_INTEGRATION.md) | Design | DEX liquidity-pool strategy and routing assumptions | | — |
| [AQUARIUS_INTEGRATION.md](AQUARIUS_INTEGRATION.md) | Design | Aquarius AMM venue (phase 2) | #656 | — |
| [PHOENIX_INTEGRATION.md](PHOENIX_INTEGRATION.md) | Design | Phoenix orderbook venue (phase 2) | #656 | — |
| [RISK_MODEL.md](RISK_MODEL.md) | Design | Weighted protocol risk scoring used before allocation | | — |
| [ML_MODEL.md](ML_MODEL.md) | Design | Off-chain APY forecasting and `submit_apy_prediction` | #650 | — |
| [REBALANCE_FAILURE_RECOVERY.md](REBALANCE_FAILURE_RECOVERY.md) | Runbook | Recovering from a `rebalance()` that fails mid-flight | | — |

## Off-chain services

| Document | Kind | What it decides or covers | Issue | Status |
|---|---|---|---|---|
| [agent/README.md](../agent/README.md) | Reference | Agent modules, scheduler, cursor persistence, graceful shutdown | #747, #750, #752 | — |
| [AGENT_DEPLOYMENT.md](AGENT_DEPLOYMENT.md) | Runbook | Deploying and operating the agent (single instance, cursor, rollout) | | — |
| [WHATSAPP_BOT.md](WHATSAPP_BOT.md) | Reference | WhatsApp bot state machine, idempotent webhooks, env, known gaps | #767, #771 | — |
| [SERVICE_API_REFERENCE.md](SERVICE_API_REFERENCE.md) | Reference | HTTP endpoints of the agent, metrics server and WhatsApp bot | | — |
| [db/README.md](../db/README.md) | Reference | Postgres/Supabase schema | | — |
| [FRONTEND_PACKAGES.md](FRONTEND_PACKAGES.md) | Decision | `frontend/` is the canonical app; `packages/vault-ui` is the library | #768 | Accepted |
| [NOTIFICATIONS.md](NOTIFICATIONS.md) | Design | Web Push, email fallback and notification preferences | #669 | — |
| [ACCESSIBILITY.md](ACCESSIBILITY.md) | Policy | WCAG 2.1 AA target and axe-core CI | #668 | — |
| [TRANSLATIONS.md](TRANSLATIONS.md) | Policy | `next-intl` translation workflow | | — |
| [LOCAL_DEV_STACK.md](LOCAL_DEV_STACK.md) | Reference | One-command local stack (`docker compose up`) | | — |

## Operations and security process

| Document | Kind | What it decides or covers | Issue | Status |
|---|---|---|---|---|
| [monitoring.md](monitoring.md) | Design | Production monitoring and audit-trail strategy | #599 | — |
| [SECURITY_MONITORING.md](SECURITY_MONITORING.md) | Design | Real-time alerting on vault events | | — |
| [INCIDENT_RESPONSE.md](INCIDENT_RESPONSE.md) | Runbook | Incident process for on-chain and operational events | | — |
| [DISASTER_RECOVERY.md](DISASTER_RECOVERY.md) | Runbook | Failover with a 5-minute RTO | #655 | — |
| [AGENT_KEY_COMPROMISE_RUNBOOK.md](AGENT_KEY_COMPROMISE_RUNBOOK.md) | Runbook | Responding to a compromised agent key | #317 | — |
| [SECRETS_HYGIENE.md](SECRETS_HYGIENE.md) | Policy | Full-history secret scan and pre-commit/CI enforcement | #605 | — |
| [BUG_BOUNTY.md](BUG_BOUNTY.md) | Policy | Bounty scope, severity rubric, safe harbour | | Active, pre-mainnet |
| [testing/test-agent-add-database-r_test_plan.md](testing/test-agent-add-database-r_test_plan.md) | Reference | Test plan for agent database repository integration tests | #863 | — |

## Adding a new decision

1. Write the document under `docs/` with a header block:

   ```markdown
   # <Title>

   > **Issue:** #NNN
   > **Status:** Proposed | Accepted | Design Decision Recorded | Superseded by <doc>
   ```

   Include **Context**, **Decision**, **Alternatives considered** and
   **Consequences** sections for Decision-kind documents.
2. Add a row to the right section of this index.
3. When a decision is replaced, mark the old document
   `Superseded by <new doc>` instead of deleting it, and update both rows here.
