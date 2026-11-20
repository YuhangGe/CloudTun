import { onMount, onUnmount, vm } from 'jinge';

import { desktopInvoke } from '@/service/desktop';
import { formatTraffic, type TrafficSnapshot, trafficRate } from '@/service/traffic';

export function ProxyTraffic() {
  const traffic = vm({ rate: 0, total: 0 });
  let previous: TrafficSnapshot | undefined;
  let timer = 0;
  let active = true;

  async function sample() {
    try {
      const snapshot = await desktopInvoke<TrafficSnapshot | null>('tauri_proxy_traffic');
      if (!active) return;
      if (snapshot) {
        traffic.rate = trafficRate(snapshot, previous);
        traffic.total = snapshot.totalBytes;
        previous = snapshot;
      } else {
        previous = undefined;
        traffic.rate = 0;
        traffic.total = 0;
      }
    } catch {
      if (active) traffic.rate = 0;
    } finally {
      if (active) timer = window.setTimeout(() => void sample(), 1000);
    }
  }
  onMount(() => void sample());
  onUnmount(() => {
    active = false;
    clearTimeout(timer);
  });

  return (
    <div
      className="hidden px-3.5 pt-1 font-mono text-[10px] text-secondary-text sm:block"
      title="实时速率 / 本次连接累计流量"
    >
      {formatTraffic(traffic.rate)}/s · {formatTraffic(traffic.total)}
    </div>
  );
}
