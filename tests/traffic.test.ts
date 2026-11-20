import { expect, test } from 'bun:test';

import { formatTraffic, trafficRate } from '../src/service/traffic';

test('traffic rate uses elapsed time and only counts bytes from the same session', () => {
  const previous = { sessionId: 1, elapsedMs: 1000, totalBytes: 44_949_000 };
  const current = { sessionId: 1, elapsedMs: 2500, totalBytes: 45_000_000 };
  expect(formatTraffic(trafficRate(current, previous))).toBe('34 KB');
  expect(formatTraffic(current.totalBytes)).toBe('45 MB');
  expect(trafficRate({ ...current, elapsedMs: 3500 }, current)).toBe(0);
  expect(trafficRate({ sessionId: 2, elapsedMs: 2500, totalBytes: 100 }, current)).toBe(0);
});
