export interface TrafficSnapshot {
  sessionId: number;
  elapsedMs: number;
  totalBytes: number;
}

export function trafficRate(current: TrafficSnapshot, previous?: TrafficSnapshot) {
  if (!previous || previous.sessionId !== current.sessionId) return 0;
  const elapsed = current.elapsedMs - previous.elapsedMs;
  return elapsed > 0 ? (Math.max(0, current.totalBytes - previous.totalBytes) * 1000) / elapsed : 0;
}

const units = ['B', 'KB', 'MB', 'GB', 'TB'];

export function formatTraffic(bytes: number) {
  let unit = 0;
  while (bytes >= 1000 && unit < units.length - 1) {
    bytes /= 1000;
    unit += 1;
  }
  const value = unit === 0 || bytes >= 10 ? Math.round(bytes) : Number(bytes.toFixed(1));
  return `${value} ${units[unit]}`;
}
