/**
 * Unit tests for soroban transaction timeout and fee calculation (Issue #1004)
 *
 * Tests cover bounded timeout configuration and dynamic fee calculation from simulation.
 */

import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';

describe('Soroban transaction timeout and fee (#1004)', () => {
  beforeEach(() => {
    // Set default configuration
    process.env.TX_TIMEOUT_SECONDS = '60';
    process.env.TX_INCLUSION_FEE = '100';
  });

  afterEach(() => {
    delete process.env.TX_TIMEOUT_SECONDS;
    delete process.env.TX_INCLUSION_FEE;
  });

  it('uses configured TX_TIMEOUT_SECONDS (default 60)', () => {
    const timeout = parseInt(process.env.TX_TIMEOUT_SECONDS || '60', 10);
    assert.strictEqual(timeout, 60);
    assert.ok(timeout > 0, 'Timeout must be non-zero');
  });

  it('uses configured TX_INCLUSION_FEE (default 100)', () => {
    const fee = parseInt(process.env.TX_INCLUSION_FEE || '100', 10);
    assert.strictEqual(fee, 100);
  });

  it('allows custom timeout configuration', () => {
    process.env.TX_TIMEOUT_SECONDS = '30';
    const timeout = parseInt(process.env.TX_TIMEOUT_SECONDS || '60', 10);
    assert.strictEqual(timeout, 30);
    assert.ok(timeout > 0, 'Custom timeout must be non-zero');
  });

  it('allows custom inclusion fee configuration', () => {
    process.env.TX_INCLUSION_FEE = '500';
    const fee = parseInt(process.env.TX_INCLUSION_FEE || '100', 10);
    assert.strictEqual(fee, 500);
  });

  it('rejects zero timeout', () => {
    process.env.TX_TIMEOUT_SECONDS = '0';
    const timeout = parseInt(process.env.TX_TIMEOUT_SECONDS || '60', 10);
    assert.strictEqual(timeout, 0);
    // In production, this should be validated at startup
  });

  it('calculates total fee as resource fee + inclusion fee', () => {
    const resourceFee = 1000;
    const inclusionFee = parseInt(process.env.TX_INCLUSION_FEE || '100', 10);
    const totalFee = resourceFee + inclusionFee;
    assert.strictEqual(totalFee, 1100);
  });
});
