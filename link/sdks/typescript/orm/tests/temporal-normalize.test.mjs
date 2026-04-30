import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  toMilliseconds,
  normalizeDateValue,
  normalizeTemporalValue,
  normalizeTimeValue,
} from '../dist/driver.js';

describe('toMilliseconds', () => {
  it('passes through millisecond values unchanged', () => {
    assert.equal(toMilliseconds(1777018016782), 1777018016782);
  });

  it('converts microseconds to milliseconds', () => {
    assert.equal(toMilliseconds(1777018016782000), 1777018016782);
  });

  it('converts seconds to milliseconds', () => {
    assert.equal(toMilliseconds(1777018016), 1777018016000);
  });

  it('passes through small numbers as-is', () => {
    assert.equal(toMilliseconds(0), 0);
    assert.equal(toMilliseconds(1000), 1000);
  });
});

describe('normalizeTemporalValue', () => {
  it('returns null/undefined unchanged', () => {
    assert.equal(normalizeTemporalValue(null), null);
    assert.equal(normalizeTemporalValue(undefined), undefined);
  });

  it('converts Date object to ISO string', () => {
    const date = new Date('2026-04-25T10:00:00.000Z');
    assert.equal(normalizeTemporalValue(date), '2026-04-25T10:00:00.000');
  });

  it('converts millisecond number to ISO string', () => {
    const result = normalizeTemporalValue(1777018016782);
    assert.equal(result, '2026-04-24T08:06:56.782');
  });

  it('converts microsecond number to ISO string', () => {
    const result = normalizeTemporalValue(1777018016782000);
    assert.equal(result, '2026-04-24T08:06:56.782');
  });

  it('converts second number to ISO string', () => {
    const result = normalizeTemporalValue(1777018016);
    assert.equal(result, '2026-04-24T08:06:56.000');
  });

  it('converts numeric string to ISO string', () => {
    const result = normalizeTemporalValue('1777018016782');
    assert.equal(result, '2026-04-24T08:06:56.782');
  });

  it('passes through non-numeric strings unchanged', () => {
    assert.equal(normalizeTemporalValue('hello'), 'hello');
    assert.equal(normalizeTemporalValue('2026-04-24T08:06:56.782Z'), '2026-04-24T08:06:56.782');
  });

  it('passes through other types unchanged', () => {
    assert.equal(normalizeTemporalValue(true), true);
    assert.deepEqual(normalizeTemporalValue({ foo: 'bar' }), { foo: 'bar' });
    assert.deepEqual(normalizeTemporalValue([1, 2]), [1, 2]);
  });

  it('handles trimmed numeric strings', () => {
    const result = normalizeTemporalValue('  1777018016782  ');
    assert.equal(result, '2026-04-24T08:06:56.782');
  });
});

describe('normalizeDateValue', () => {
  it('converts Date32 day offsets to ISO dates', () => {
    assert.equal(normalizeDateValue(20573), '2026-04-30');
  });

  it('passes through ISO date strings as dates', () => {
    assert.equal(normalizeDateValue('2026-04-30T12:34:56Z'), '2026-04-30');
  });
});

describe('normalizeTimeValue', () => {
  it('converts microseconds since midnight to HH:mm:ss', () => {
    assert.equal(normalizeTimeValue(45_296_123_456), '12:34:56.123456');
  });

  it('passes through time strings', () => {
    assert.equal(normalizeTimeValue('12:34:56'), '12:34:56');
  });
});
