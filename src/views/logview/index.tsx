import { cx, onMount, onUnmount, ref, vm, watch } from 'jinge';

import { type Log, logStore, refreshLogs } from '@/store/log';

interface LogRow extends Log {
  time: string;
  fullTime: string;
  isoTime: string;
  category: string;
  content: string;
  tone: 'normal' | 'warning' | 'error';
}
const categories: Record<string, string> = {
  cloud: '云主机',
  startup: '启动',
  geo: 'GEO',
  proxy: '代理',
  agent: '云主机',
  info: '系统',
  error: '错误',
  warn: '警告',
  ping: '心跳',
  disconnected: '连接',
};
function row(log: Log): LogRow {
  const prefix = /^\[([^\]]+)\]\s*/.exec(log.text);
  const parts = prefix?.[1].split('::') ?? [];
  const category = parts[0] === 'log' ? parts[1] : parts[0];
  const date = new Date(log.timestamp);
  const pad = (value: number, length = 2) => String(value).padStart(length, '0');
  return {
    ...log,
    time: `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${pad(date.getMilliseconds(), 3)}`,
    fullTime: date.toLocaleString(),
    isoTime: date.toISOString(),
    category: categories[category ?? 'info'] ?? category ?? '系统',
    content: prefix ? log.text.slice(prefix[0].length) : log.text,
    tone: /error|失败|超时|失联/i.test(log.text)
      ? 'error'
      : /warn|警告/i.test(log.text)
        ? 'warning'
        : 'normal',
  };
}
export function LogView() {
  const el = ref<HTMLDivElement>();
  const state = vm({ rows: [] as LogRow[], atBottom: true });
  let cached = new Map<number, LogRow>();
  let frame = 0;
  let scrollRevision = 0;
  function updatePosition() {
    const panel = el.value;
    if (panel) state.atBottom = panel.scrollHeight - panel.clientHeight - panel.scrollTop <= 4;
  }
  function anchor() {
    const panel = el.value;
    if (!panel) return;
    const top = panel.getBoundingClientRect().top;
    const first = [...panel.querySelectorAll<HTMLElement>('[data-log-id]')].find(
      (item) => item.getBoundingClientRect().bottom > top,
    );
    return first
      ? { id: first.dataset.logId, offset: first.getBoundingClientRect().top - top }
      : undefined;
  }
  function scrollToLatest() {
    state.atBottom = true;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      if (el.value && state.atBottom) el.value.scrollTop = el.value.scrollHeight;
    });
  }
  watch(
    logStore,
    'logs',
    (logs) => {
      const saved = state.atBottom ? undefined : anchor();
      const revision = scrollRevision;
      const next = new Map<number, LogRow>();
      state.rows = logs.map((log) => {
        const previous = cached.get(log.id);
        const item =
          previous?.timestamp === log.timestamp && previous.text === log.text ? previous : row(log);
        next.set(log.id, item);
        return item;
      });
      cached = next;
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        const panel = el.value;
        if (!panel) return;
        if (state.atBottom) panel.scrollTop = panel.scrollHeight;
        else if (saved && revision === scrollRevision) {
          // Retaining only 1000 entries may remove rows above the visible reading position.
          const item = [...panel.querySelectorAll<HTMLElement>('[data-log-id]')].find(
            (item) => item.dataset.logId === saved.id,
          );
          if (item)
            panel.scrollTop +=
              item.getBoundingClientRect().top - panel.getBoundingClientRect().top - saved.offset;
          else panel.scrollTop = 0;
        }
        updatePosition();
      });
    },
    { immediate: true },
  );
  onMount(() => {
    void refreshLogs();
    scrollToLatest();
  });
  onUnmount(() => cancelAnimationFrame(frame));
  return (
    <section className="mx-auto max-w-[1100px]">
      <div className="mb-7 flex items-center justify-between gap-4">
        <h1 className="m-0 text-2xl font-semibold tracking-tight">运行日志</h1>
        <button
          type="button"
          title="滚动到底部"
          aria-label="滚动到底部"
          disabled={state.atBottom}
          className="grid size-8 shrink-0 cursor-pointer place-items-center rounded-lg border border-border bg-transparent text-base text-secondary-text hover:text-primary disabled:cursor-default disabled:opacity-30"
          on:click={scrollToLatest}
        >
          <span className="icon-[ant-design--vertical-align-bottom-outlined]" />
        </button>
      </div>
      {logStore.error && (
        <p className="mb-3 text-xs text-error" role="alert">
          {logStore.error}
        </p>
      )}
      <div
        className="h-[calc(100dvh-210px)] min-h-40 overflow-auto overscroll-contain rounded-xl border border-border bg-primary/[0.015] [overflow-anchor:none]"
        ref={el}
        role="log"
        aria-label="运行日志列表"
        on:scroll={() => {
          ++scrollRevision;
          updatePosition();
        }}
      >
        {!logStore.ready ? (
          <p className="p-4 text-xs text-secondary-text">正在读取后台日志…</p>
        ) : !state.rows.length ? (
          <p className="p-4 text-xs text-secondary-text">暂无日志</p>
        ) : (
          <div className="divide-y divide-border/50">
            {state.rows.map((log) => (
              <div
                key={log.id}
                data-log-id={log.id}
                className="grid grid-cols-[5.5rem_3.5rem_minmax(0,1fr)] items-start gap-x-3 px-4 py-2.5 text-xs leading-5 max-sm:grid-cols-[4.75rem_minmax(0,1fr)] max-sm:gap-x-2 max-sm:px-3"
              >
                <time
                  className="font-mono text-[10px] text-secondary-text/65"
                  title={log.fullTime}
                  dateTime={log.isoTime}
                >
                  {log.time}
                </time>
                <span
                  className={cx(
                    'mt-0.5 truncate rounded px-1.5 text-center text-[10px] leading-4 max-sm:hidden',
                    log.tone === 'error'
                      ? 'bg-error/8 text-error'
                      : log.tone === 'warning'
                        ? 'bg-amber-500/10 text-amber-700'
                        : 'bg-primary/5 text-secondary-text',
                  )}
                >
                  {log.category}
                </span>
                <p
                  className={cx(
                    'min-w-0 font-mono wrap-anywhere whitespace-pre-wrap',
                    log.tone === 'error'
                      ? 'text-error'
                      : log.tone === 'warning'
                        ? 'text-amber-700'
                        : 'text-normal-text/80',
                  )}
                >
                  {log.content}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}
