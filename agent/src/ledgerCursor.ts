/**
 * Persisted event cursor for the event listener (Issue #747).
 *
 * The listener's position is saved after every processed event so a restarted
 * agent resumes where it stopped instead of jumping to the latest ledger and
 * silently missing deposits/withdrawals in between.
 *
 * The position is the Soroban RPC paging token (an event id or a page
 * `cursor`), which is exact even when a page ends in the middle of a ledger.
 * `startLedger` is only used on the very first run.
 *
 * The id of the last handled event is stored alongside the paging token.
 * Event ids are ordered, so any event at or before it is skipped: a replayed
 * page (stale in-memory position, RPC returning overlap, or a rewind after a
 * failed write) never re-runs an event that was already committed. A failed
 * write is kept as pending and retried by `flush()` (each tick and on
 * shutdown) rather than silently dropped.
 */

import { FileStateStore, PostgresStateStore, StateStore } from './stateBackup';
import logger from './logger';

export const LEDGER_CURSOR_KEY = 'agent:ledger_cursor';

export interface LedgerCursorValue {
  /** RPC paging token to resume from (exclusive). */
  pagingToken: string;
  /** Ledger of the last processed position, for logs / diagnostics. */
  ledger: number;
  /** Id of the last event whose handling completed; absent before the first event. */
  lastEventId?: string;
  updatedAt: number;
}

/**
 * Orders Soroban event ids (`<19-digit TOID>-<10-digit index>`). Falls back to
 * numeric comparison of each part so ids of differing widths still compare
 * correctly.
 */
export function compareEventIds(a: string, b: string): number {
  if (a.length === b.length) return a < b ? -1 : a > b ? 1 : 0;
  const pa = a.split('-');
  const pb = b.split('-');
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const x = BigInt(pa[i] ?? '0');
    const y = BigInt(pb[i] ?? '0');
    if (x !== y) return x < y ? -1 : 1;
  }
  return 0;
}

export type EventPosition = { cursor: string } | { startLedger: number };

type Queryable = { query: (sql: string, values?: unknown[]) => Promise<unknown> };

/** Postgres store that creates the `agent_state` table on first use. */
class EnsuredPostgresStore implements StateStore<LedgerCursorValue> {
  private ready: Promise<unknown> | null = null;
  private readonly inner: PostgresStateStore<LedgerCursorValue>;

  constructor(private readonly pool: Queryable) {
    this.inner = new PostgresStateStore<LedgerCursorValue>(pool);
  }

  private ensureTable(): Promise<unknown> {
    if (!this.ready) {
      this.ready = this.pool
        .query(
          `CREATE TABLE IF NOT EXISTS agent_state (
             key TEXT PRIMARY KEY,
             value JSONB NOT NULL,
             updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
           )`,
        )
        .catch((err) => {
          this.ready = null;
          throw err;
        });
    }
    return this.ready;
  }

  async save(key: string, value: LedgerCursorValue): Promise<void> {
    await this.ensureTable();
    await this.inner.save(key, value);
  }

  async load(key: string): Promise<LedgerCursorValue | null> {
    await this.ensureTable();
    return this.inner.load(key);
  }
}

/** Postgres when DATABASE_URL is set, otherwise a local JSON file. */
export function createLedgerCursorStore(pool: Queryable): StateStore<LedgerCursorValue> {
  if (process.env.DATABASE_URL) {
    return new EnsuredPostgresStore(pool);
  }
  const file = process.env.LEDGER_CURSOR_FILE || '.agent-state/ledger-cursor.json';
  logger.warn({ file }, 'DATABASE_URL not set; persisting ledger cursor to a local file');
  return new FileStateStore<LedgerCursorValue>(file);
}

export class LedgerCursor {
  private lastSavedToken: string | null = null;
  private lastEventId: string | null = null;
  private pending: LedgerCursorValue | null = null;

  constructor(
    private readonly store: StateStore<LedgerCursorValue>,
    private readonly now: () => number = () => Date.now(),
  ) {}

  /** Returns the stored position, or null when none (or unreadable). */
  async load(): Promise<LedgerCursorValue | null> {
    try {
      const value = await this.store.load(LEDGER_CURSOR_KEY);
      if (value && typeof value.pagingToken === 'string' && value.pagingToken.length > 0) {
        this.lastSavedToken = value.pagingToken;
        this.lastEventId = typeof value.lastEventId === 'string' ? value.lastEventId : null;
        return value;
      }
    } catch (error) {
      logger.error({ error: error instanceof Error ? error.message : error }, 'Failed to load ledger cursor');
      throw error;
    }
    return null;
  }

  /** True when the event was already handled (at or before the last committed event). */
  isProcessed(eventId: string): boolean {
    return this.lastEventId !== null && compareEventIds(eventId, this.lastEventId) <= 0;
  }

  /** True when a position has been recorded in memory but not yet persisted. */
  get hasPendingWrite(): boolean {
    return this.pending !== null;
  }

  /**
   * Records a new position and tries to persist it. Pass `eventId` when the
   * position follows a handled event. Returns false (and keeps the position
   * pending for `flush`) when the write fails; never throws.
   */
  async advance(pagingToken: string, ledger: number, eventId?: string): Promise<boolean> {
    if (!pagingToken) return true;
    if (eventId && (this.lastEventId === null || compareEventIds(eventId, this.lastEventId) > 0)) {
      this.lastEventId = eventId;
    }
    if (pagingToken === this.lastSavedToken && this.pending === null) return true;
    this.pending = {
      pagingToken,
      ledger,
      ...(this.lastEventId ? { lastEventId: this.lastEventId } : {}),
      updatedAt: this.now(),
    };
    return this.flush();
  }

  /** Persists any pending position. Returns true when nothing is left pending. */
  async flush(): Promise<boolean> {
    const value = this.pending;
    if (!value) return true;
    try {
      await this.store.save(LEDGER_CURSOR_KEY, value);
      this.lastSavedToken = value.pagingToken;
      if (this.pending === value) this.pending = null;
      return true;
    } catch (error) {
      logger.error(
        { error: error instanceof Error ? error.message : error, pagingToken: value.pagingToken },
        'Failed to persist ledger cursor; will retry',
      );
      return false;
    }
  }

  /** Persists the position if it changed. Errors are logged, not thrown. */
  async save(pagingToken: string, ledger: number): Promise<void> {
    await this.advance(pagingToken, ledger);
  }
}

/** Resume from the saved paging token, or start at `latestLedger` on first run. */
export function initialPosition(stored: LedgerCursorValue | null, latestLedger: number): EventPosition {
  return stored ? { cursor: stored.pagingToken } : { startLedger: latestLedger };
}

/**
 * True when the RPC rejected the request because the saved position is no
 * longer inside its retention window (or otherwise unusable).
 */
export function isStalePositionError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /cursor|startLedger|ledger range|out of range|retention/i.test(message);
}
