import { rpc } from '@stellar/stellar-sdk';
import { pool } from './db';
import { evaluateYield } from './yieldComparison';
import { processEventForAlerts } from './alertEngine';
import { AlertEventPayload, AlertEventType } from './alertEventPayload';
import logger from './logger';
import { withRetry } from './retry';
import { createLedgerCursorStore, EventPosition, initialPosition, isStalePositionError, LedgerCursor } from './ledgerCursor';
import { decodeVaultTransfer, vaultEventType } from './vaultEventPayload';
import { DEFAULT_STRATEGY, getCurrentAllocation, getUserStrategy } from './userStrategies';
import { submitRebalanceTx } from './sorobanTx';
import { createDecisionId } from './rebalanceOperations';

import { EventCircuitBreaker, CircuitBreakerStatus } from './eventCircuitBreaker';

export { pool };

const rpcUrl = process.env.SOROBAN_RPC_URL || 'https://soroban-testnet.stellar.org';
export const server = new rpc.Server(rpcUrl);

const EVENTS_PAGE_LIMIT = 100;
const MAX_PAGES_PER_TICK = 10;

export const eventCircuitBreaker = new EventCircuitBreaker();

export function getCircuitBreakerStatus(): CircuitBreakerStatus {
  return eventCircuitBreaker.getStatus();
}

export function resetCircuitBreaker(): void {
  eventCircuitBreaker.reset();
}

/** Raised when the cursor cannot be persisted; the tick stops so no further events run ahead of it. */
class CursorPersistError extends Error {
  constructor() {
    super('Ledger cursor could not be persisted');
    this.name = 'CursorPersistError';
  }
}

let pollTimer: NodeJS.Timeout | null = null;
let isStopped = true;
let activePoll: Promise<void> | null = null;
let activeCursor: LedgerCursor | null = null;

/**
 * Stops the listener gracefully: no new tick is scheduled, an in-flight tick
 * finishes the event it is handling (it checks `isStopped` between events),
 * and the latest position is flushed so a restart resumes exactly after the
 * last handled event. Waits at most `timeoutMs` for the in-flight tick.
 */
export async function stopEventListener(timeoutMs = 5000): Promise<void> {
  isStopped = true;
  if (pollTimer) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }

  if (activePoll) {
    logger.info({ timeoutMs }, 'Waiting for in-flight event polling to complete');
    let timer: NodeJS.Timeout | null = null;
    const timedOut = new Promise<'timeout'>((resolve) => {
      timer = setTimeout(() => resolve('timeout'), timeoutMs);
    });
    try {
      const outcome = await Promise.race([activePoll.then(() => 'done' as const), timedOut]);
      if (outcome === 'timeout') {
        logger.warn({ timeoutMs }, 'In-flight event poll did not finish before shutdown timeout');
      }
    } finally {
      if (timer) clearTimeout(timer);
    }
  }

  const cursor = activeCursor;
  activeCursor = null;
  if (cursor?.hasPendingWrite && !(await cursor.flush())) {
    logger.error('Ledger cursor still unpersisted at shutdown; the last handled event(s) may be replayed on restart');
  }
  logger.info('Event listener stopped');
}

/**
 * Listens for on-chain deposit and withdraw events from the vault contract.
 * Detects new deposits within 5 seconds and triggers yield deployment.
 * Protects RPC with exponential backoff and circuit-breaking on sustained failures.
 *
 * Delivery is effectively once per event: the cursor (paging token plus last
 * handled event id) is persisted after every event, already-handled events
 * are skipped on replay, and processing halts while the cursor cannot be
 * written. Only a hard crash between handling an event and persisting the
 * cursor can replay that single event.
 */
export async function startEventListener() {
  if (!process.env.VAULT_CONTRACT_ID) {
    logger.warn('VAULT_CONTRACT_ID is not set. Event listener requires a contract ID to monitor.');
    return;
  }

  try {
    const latestLedgerResponse = await withRetry(
      () => server.getLatestLedger(),
      'getLatestLedger',
    );
    const cursor = new LedgerCursor(createLedgerCursorStore(pool));
    // If the saved cursor cannot be read we refuse to start rather than
    // silently skipping to the latest ledger.
    const stored = await withRetry(() => cursor.load(), 'loadLedgerCursor');
    const startSequence = process.env.START_LEDGER
      ? parseInt(process.env.START_LEDGER, 10)
      : latestLedgerResponse.sequence;
    let position: EventPosition = initialPosition(stored, startSequence);
    if (stored) {
      logger.info({ pagingToken: stored.pagingToken, ledger: stored.ledger, latestLedger: latestLedgerResponse.sequence }, 'Resuming event listener from saved cursor');
    } else {
      logger.info({ startLedger: startSequence }, 'Starting event listener from initial ledger (no saved cursor)');
    }

    const filters: rpc.Api.EventFilter[] = [{ type: 'contract', contractIds: [process.env.VAULT_CONTRACT_ID!] }];
    activeCursor = cursor;
    isStopped = false;

    const scheduleNextTick = (delayMs: number) => {
      if (isStopped) return;
      if (pollTimer) clearTimeout(pollTimer);
      pollTimer = setTimeout(() => {
        pollTimer = null;
        // A slow poll must not overlap the next tick, or the same page would
        // be processed twice.
        if (isStopped || activePoll) return;
        activePoll = pollTick().finally(() => {
          activePoll = null;
        });
      }, delayMs);
    };

    const pollPages = async () => {
      // A write that failed last tick must land before anything new is
      // handled, so the durable cursor never falls more than one event behind.
      if (!(await cursor.flush())) throw new CursorPersistError();

      let pageCount = 0;
      while (pageCount < MAX_PAGES_PER_TICK && !isStopped) {
        let response: rpc.Api.GetEventsResponse;
        try {
          const request: rpc.Api.GetEventsRequest = 'cursor' in position
            ? { filters, cursor: position.cursor, limit: EVENTS_PAGE_LIMIT }
            : { filters, startLedger: position.startLedger, limit: EVENTS_PAGE_LIMIT };
          response = await withRetry(() => server.getEvents(request), 'getEvents');
        } catch (error) {
          if ('cursor' in position && isStalePositionError(error)) {
            const latest = await withRetry(() => server.getLatestLedger(), 'getLatestLedger');
            logger.error(
              { pagingToken: position.cursor, resumeLedger: latest.sequence, error: error instanceof Error ? error.message : error },
              'Saved event cursor is outside the RPC retention window; events in the gap were MISSED and need manual backfill',
            );
            position = { startLedger: latest.sequence };
            return; // retry from the tip on the next tick
          }
          throw error;
        }

        for (const event of response.events) {
          if (isStopped) return;

          if (cursor.isProcessed(event.id)) {
            logger.debug({ eventId: event.id }, 'Skipping already-processed event');
          } else {
            const eventType = vaultEventType(event.topic);
            if (eventType) {
              await handleVaultEvent(eventType, event);
            }
          }

          // Event ids are paging tokens: persist after each event so a
          // restart resumes right after the last handled one.
          position = { cursor: event.id };
          if (!(await cursor.advance(event.id, event.ledger, event.id))) throw new CursorPersistError();
        }

        // The page cursor also covers the scanned range with no matching
        // events, so idle periods are not rescanned after a restart.
        if (response.cursor && !isStopped) {
          position = { cursor: response.cursor };
          if (!(await cursor.advance(response.cursor, response.latestLedger))) throw new CursorPersistError();
        }

        if (response.events.length < EVENTS_PAGE_LIMIT) return;
        pageCount++;
      }
    };

    const pollTick = async () => {
      if (!eventCircuitBreaker.canExecute()) {
        const backoffDelay = eventCircuitBreaker.getNextDelayMs();
        logger.warn(
          { state: eventCircuitBreaker.getState(), nextDelayMs: backoffDelay },
          'Soroban event polling skipped: circuit breaker open/in cooldown',
        );
        scheduleNextTick(backoffDelay);
        return;
      }

      try {
        await pollPages();
        eventCircuitBreaker.recordSuccess();
        scheduleNextTick(eventCircuitBreaker.getNextDelayMs());
      } catch (error) {
        eventCircuitBreaker.recordFailure(error);
        const nextDelay = eventCircuitBreaker.getNextDelayMs();
        logger.error(
          {
            error: error instanceof Error ? error.message : error,
            nextPollInMs: nextDelay,
            circuitState: eventCircuitBreaker.getState(),
          },
          'Error polling Soroban events; backing off',
        );
        scheduleNextTick(nextDelay);
      }
    };

    scheduleNextTick(eventCircuitBreaker.getNextDelayMs());
  } catch (error) {
    eventCircuitBreaker.recordFailure(error);
    logger.error({ error: error instanceof Error ? error.message : error }, 'Failed to initialize event listener');
  }
}

async function handleVaultEvent(eventType: AlertEventType, event: rpc.Api.EventResponse) {
  logger.info({ eventType, ledger: event.ledger }, 'Detected event');

  await logEventToDb(eventType, event.id, event.ledger);

  const payload = decodeVaultTransfer(event);
  if (!payload) {
    logger.warn({ eventType, eventId: event.id }, 'Could not decode amount from vault event; skipping alert evaluation');
  }

  if (eventType === 'deposit') {
    logger.info('New deposit detected, evaluating yield');
    let userStrategy: string = DEFAULT_STRATEGY;
    let currentProtocol = 'none';
    let currentApy = 0;
    if (process.env.DATABASE_URL) {
      try {
        if (payload?.user) userStrategy = await getUserStrategy(pool, payload.user);
        ({ protocol: currentProtocol, apy: currentApy } = await getCurrentAllocation(pool));
      } catch (error) {
        logger.error({ error: error instanceof Error ? error.message : error }, 'Failed to load user strategy; using defaults');
      }
    }

    // A yield-evaluation failure must not block alerting or stall the cursor
    // on this event forever.
    try {
      const decision = await evaluateYield(userStrategy, currentProtocol, currentApy);

      if (decision.shouldRebalance && decision.targetProtocol) {
        logger.info({ targetProtocol: decision.targetProtocol, userStrategy }, 'Rebalance needed');
        const expectedApy = currentApy; // use current evaluated APY or fetch it
        const snapshot = {
          eventId: event.id,
          strategy: userStrategy,
          currentProtocol,
          currentApy,
          decision,
        };
        await submitRebalanceTx(decision.targetProtocol, expectedApy, {
          decisionId: createDecisionId('deposit-event', event.id, snapshot),
          snapshot,
        });
      }
    } catch (error) {
      logger.error({ error: error instanceof Error ? error.message : error, userStrategy }, 'Yield evaluation failed for deposit');
    }
  }

  if (payload) {
    const alertPayload: AlertEventPayload = {
      type: eventType,
      amount: payload.amount,
      user: payload.user,
      ledger: event.ledger,
    };
    await processEventForAlerts(alertPayload);
  }
}

async function logEventToDb(type: string, eventId: string, ledger: number) {
  try {
    if (process.env.DATABASE_URL) {
      await pool.query(
        'INSERT INTO vault_events (event_id, event_type, ledger_sequence, created_at) VALUES ($1, $2, $3, NOW()) ON CONFLICT DO NOTHING',
        [eventId, type, ledger]
      );
    }
  } catch (error) {
    logger.error({ error: error instanceof Error ? error.message : error }, 'Database logging failed');
  }
}
