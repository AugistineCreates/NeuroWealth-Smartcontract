/**
 * Durable ledger cursor + graceful shutdown tests for the event listener.
 *
 * Drives startEventListener/stopEventListener against a stubbed RPC "chain"
 * and a fake pool that emulates the agent_state table, so restarts can be
 * simulated by stopping and starting the listener over the same stored
 * cursor. Every handled vault event is observed through its vault_events
 * insert, which lets the tests assert each event is handled exactly once.
 */

// Fast ticks; must be set before the circuit breaker is constructed.
process.env.SOROBAN_POLL_INTERVAL_MS = '20';
process.env.SOROBAN_CIRCUIT_BREAKER_THRESHOLD = '1000';
process.env.SOROBAN_POLL_MAX_BACKOFF_MS = '40';

import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { startEventListener, stopEventListener, server, resetCircuitBreaker } from './eventListener';
import { getPool } from './db';
import { LEDGER_CURSOR_KEY, LedgerCursorValue } from './ledgerCursor';

interface ChainEvent {
  id: string;
  ledger: number;
  topic: unknown[];
  value: unknown;
}

function eventId(ledger: number, index: number): string {
  return `${String(ledger).padStart(19, '0')}-${String(index).padStart(10, '0')}`;
}

function withdrawEvent(ledger: number, index = 1): ChainEvent {
  return { id: eventId(ledger, index), ledger, topic: ['withdraw', 'GUSER'], value: null };
}

// ── Stubbed RPC chain ────────────────────────────────────────────────────────
let chain: ChainEvent[] = [];
let latestLedger = 100;
/** When true, getEvents ignores the cursor and returns the whole chain (overlapping pages). */
let replayEverything = false;

const serverAny = server as unknown as Record<string, unknown>;
serverAny.getLatestLedger = async () => ({ sequence: latestLedger });
serverAny.getEvents = async (req: { cursor?: string; startLedger?: number; limit?: number }) => {
  const limit = req.limit ?? 100;
  let events: ChainEvent[];
  if (replayEverything) {
    events = chain;
  } else if (req.cursor !== undefined) {
    const after = req.cursor;
    events = chain.filter((e) => e.id > after);
  } else {
    events = chain.filter((e) => e.ledger >= (req.startLedger ?? 0));
  }
  events = events.slice(0, limit);
  return {
    events,
    cursor: events.length > 0 ? events[events.length - 1].id : req.cursor,
    latestLedger,
  };
};

// ── Fake pool: agent_state + vault_events ────────────────────────────────────
let agentState = new Map<string, LedgerCursorValue>();
let handled: string[] = [];
let failCursorWrites = 0;
let slowEventMs = 0;
let onEventStart: ((id: string) => void) | null = null;

// `pool` is a read-through proxy, so patch the live instance behind it.
const poolAny = getPool() as unknown as Record<string, unknown>;
poolAny.query = async (sql: string, values: unknown[] = []) => {
  const normalized = sql.replace(/\s+/g, ' ').trim().toUpperCase();

  if (normalized.startsWith('INSERT INTO AGENT_STATE')) {
    if (failCursorWrites > 0) {
      failCursorWrites--;
      throw new Error('connection terminated unexpectedly');
    }
    agentState.set(values[0] as string, JSON.parse(values[1] as string));
    return { rows: [] };
  }
  if (normalized.startsWith('SELECT VALUE FROM AGENT_STATE')) {
    const value = agentState.get(values[0] as string);
    return { rows: value ? [{ value }] : [] };
  }
  if (normalized.startsWith('INSERT INTO VAULT_EVENTS')) {
    const id = values[0] as string;
    onEventStart?.(id);
    if (slowEventMs > 0) await new Promise((r) => setTimeout(r, slowEventMs));
    handled.push(id);
    return { rows: [] };
  }
  return { rows: [] };
};

async function waitFor(predicate: () => boolean, timeoutMs = 3000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate()) {
    if (Date.now() > deadline) throw new Error('waitFor timed out');
    await new Promise((r) => setTimeout(r, 5));
  }
}

function durableCursor(): LedgerCursorValue | undefined {
  return agentState.get(LEDGER_CURSOR_KEY);
}

function countOf(id: string): number {
  return handled.filter((h) => h === id).length;
}

describe('event listener – durable cursor across restarts', () => {
  beforeEach(() => {
    process.env.VAULT_CONTRACT_ID = 'CONTRACT_ABC';
    process.env.DATABASE_URL = 'postgres://localhost/test';
    delete process.env.START_LEDGER;
    chain = [];
    latestLedger = 100;
    replayEverything = false;
    agentState = new Map();
    handled = [];
    failCursorWrites = 0;
    slowEventMs = 0;
    onEventStart = null;
    resetCircuitBreaker();
  });

  afterEach(async () => {
    await stopEventListener(1000);
  });

  it('resumes after a restart without reprocessing or skipping events', async () => {
    chain = [withdrawEvent(100), withdrawEvent(101)];
    await startEventListener();
    await waitFor(() => handled.length === 2);
    await stopEventListener(1000);
    assert.strictEqual(durableCursor()?.lastEventId, eventId(101, 1));

    // Events land while the agent is down; the tip moves far past them.
    chain.push(withdrawEvent(105), withdrawEvent(106));
    latestLedger = 500;

    await startEventListener();
    await waitFor(() => handled.length === 4);
    await new Promise((r) => setTimeout(r, 100)); // a few more idle ticks

    assert.deepStrictEqual(handled, [
      eventId(100, 1),
      eventId(101, 1),
      eventId(105, 1),
      eventId(106, 1),
    ]);
    assert.strictEqual(durableCursor()?.pagingToken, eventId(106, 1));
  });

  it('skips already-handled events when the RPC replays them', async () => {
    chain = [withdrawEvent(100), withdrawEvent(101)];
    replayEverything = true;
    await startEventListener();
    await waitFor(() => handled.length >= 2);
    await new Promise((r) => setTimeout(r, 150)); // many overlapping ticks

    chain.push(withdrawEvent(102));
    await waitFor(() => handled.length >= 3);
    await new Promise((r) => setTimeout(r, 100));

    for (const e of chain) assert.strictEqual(countOf(e.id), 1, `event ${e.id} handled more than once`);
  });

  it('halts while the cursor cannot be persisted, then resumes exactly once per event', async () => {
    chain = [withdrawEvent(100), withdrawEvent(101), withdrawEvent(102)];
    failCursorWrites = 3; // first save fails, plus two retry flushes
    await startEventListener();

    await waitFor(() => handled.length >= 1);
    await new Promise((r) => setTimeout(r, 30));
    // No event may run ahead of an unpersisted cursor.
    if (failCursorWrites > 0) assert.strictEqual(handled.length, 1);

    await waitFor(() => handled.length === 3);
    await waitFor(() => durableCursor()?.lastEventId === eventId(102, 1));
    for (const e of chain) assert.strictEqual(countOf(e.id), 1);
  });

  it('a restart after a failed cursor write does not replay the event once flushed on shutdown', async () => {
    chain = [withdrawEvent(100)];
    // Every write during the run fails, but the database is back by shutdown.
    failCursorWrites = Number.MAX_SAFE_INTEGER;
    await startEventListener();
    await waitFor(() => handled.length === 1);
    failCursorWrites = 0;
    await stopEventListener(1000);
    assert.strictEqual(durableCursor()?.lastEventId, eventId(100, 1));

    await startEventListener();
    await new Promise((r) => setTimeout(r, 100));
    assert.strictEqual(countOf(eventId(100, 1)), 1);
  });

  it('graceful shutdown finishes the in-flight event and persists it before returning', async () => {
    chain = [withdrawEvent(100), withdrawEvent(101), withdrawEvent(102)];
    slowEventMs = 150;
    let started = false;
    onEventStart = () => {
      started = true;
    };
    await startEventListener();
    await waitFor(() => started);

    await stopEventListener(2000);

    // The event that was mid-flight completed and is durable; nothing after it ran.
    assert.strictEqual(handled.length, 1);
    assert.strictEqual(durableCursor()?.lastEventId, handled[0]);

    // Restart picks up with the next event, no duplicates, no gaps.
    slowEventMs = 0;
    await startEventListener();
    await waitFor(() => handled.length === 3);
    assert.deepStrictEqual(handled, chain.map((e) => e.id));
  });

  it('does not schedule further polls after stopping', async () => {
    await startEventListener();
    await stopEventListener(1000);
    chain = [withdrawEvent(100)];
    await new Promise((r) => setTimeout(r, 100));
    assert.strictEqual(handled.length, 0);
  });
});
