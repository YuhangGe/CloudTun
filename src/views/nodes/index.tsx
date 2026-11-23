import { cx, onMount, vm, watch } from 'jinge';
import { Button } from 'jinge-antd';

import { SubscriptionShare } from '@/components/SubscriptionShare';
import { IS_IOS } from '@/service/util';
import {
  desktop,
  updateSubscription,
  testNodes,
  testNodeLatency,
  connectSelected,
  disconnectProxy,
  selectNode,
  toggleAutoSwitch,
  toggleAutoSwitchCandidate,
  type NodeEntry,
} from '@/store/desktop';
import { globalSettings } from '@/store/settings';
import { activateInitialSubscription } from '@/store/startup';

function updatedText(at: number) {
  return `上次更新 ${new Date(at).toLocaleString()}`;
}
function latencyText(ms: number | null, tested: boolean) {
  return !tested ? '待测试' : ms === null ? '测试失败 / 超时' : `${ms} ms`;
}
export function NodesView() {
  onMount(() => {
    void activateInitialSubscription();
  });
  const state = vm({
    url: desktop.subscriptionUrl,
    sortByLatency: false,
    visible: [] as NodeEntry[],
    available: 0,
    selectedName: '',
    selectedNodeId: '',
    selectedLatency: '',
    canConnect: false,
  });
  function derive() {
    state.visible = [...desktop.nodes];
    if (state.sortByLatency) {
      const score = (node: NodeEntry) =>
        !node.unsupportedReason && node.tested && node.latencyMs !== null
          ? node.latencyMs
          : Infinity;
      state.visible.sort((a, b) => (score(a) === score(b) ? 0 : score(a) - score(b)));
    }
    state.available = desktop.nodes.filter(
      (node) => !node.unsupportedReason && node.latencyMs !== null && node.latencyMs < 5000,
    ).length;
    const selected = desktop.nodes.find(
      (node) => node.id === globalSettings.subscriptionSelectedId,
    );
    state.selectedName = selected?.name ?? '尚未选择节点';
    state.selectedNodeId = selected && !selected.unsupportedReason ? selected.id : '';
    state.selectedLatency = selected
      ? `当前节点延迟 ${latencyText(selected.latencyMs, selected.tested)}`
      : '';
    state.canConnect = Boolean(
      selected &&
      !selected.unsupportedReason &&
      selected.latencyMs !== null &&
      selected.latencyMs < 5000,
    );
  }
  watch(desktop, 'subscriptionUrl', (url) => {
    state.url = url;
  });
  watch(desktop, 'nodes', derive, { immediate: true, deep: true });
  watch(globalSettings, 'subscriptionSelectedId', derive);
  watch(state, 'sortByLatency', derive);
  return (
    <div className="mx-auto max-w-[1100px]">
      {IS_IOS && (
        <p className="mb-4 rounded-lg border border-primary/20 bg-primary/5 p-3 text-xs leading-relaxed text-secondary-text">
          iOS 订阅连接将在后续版本接入。
        </p>
      )}
      <section className="flex flex-wrap items-center gap-3 rounded-2xl border border-primary/15 bg-linear-to-br from-primary/5 to-cyan-400/5 p-4 sm:flex-nowrap sm:gap-4 sm:px-6 sm:py-5.5">
        <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-primary/10 text-2xl text-primary sm:size-12.5">
          <span className="icon-[ant-design--api-outlined]" />
        </div>
        <div className="flex min-w-0 flex-1 flex-col justify-center gap-1 [&_h1]:m-0 [&_h1]:truncate [&_h1]:text-lg [&_h1]:leading-6 [&_h1]:font-semibold [&_h1]:tracking-tight sm:[&_h1]:text-[21px] sm:[&_h1]:leading-7">
          <h1>
            {desktop.status.running && desktop.status.source === 'subscription'
              ? desktop.status.name
              : state.selectedName}
          </h1>
          <div className="flex min-h-5 items-center gap-1.5">
            <p className="text-xs leading-5 text-secondary-text">
              {desktop.connecting
                ? '正在验证节点并建立代理…'
                : state.selectedNodeId
                  ? desktop.testingNodeId === state.selectedNodeId
                    ? '当前节点测速中…'
                    : state.selectedLatency
                  : '选择一个节点，连接你的网络'}
            </p>
            {state.selectedNodeId && !IS_IOS && (
              <button
                type="button"
                aria-label="刷新当前节点延迟"
                title="刷新当前节点延迟"
                aria-busy={desktop.testingNodeId === state.selectedNodeId}
                className="grid size-5 shrink-0 cursor-pointer place-items-center rounded text-xs text-secondary-text hover:text-primary disabled:cursor-default disabled:opacity-40"
                disabled={
                  desktop.testing ||
                  Boolean(desktop.testingNodeId) ||
                  desktop.connecting ||
                  desktop.updating
                }
                on:click={() => {
                  void testNodeLatency(state.selectedNodeId);
                }}
              >
                <span
                  className={cx(
                    'icon-[ant-design--reload-outlined]',
                    desktop.testingNodeId === state.selectedNodeId && 'animate-spin',
                  )}
                />
              </button>
            )}
          </div>
        </div>
        <Button
          type={
            desktop.status.running && desktop.status.source === 'subscription'
              ? 'default'
              : 'primary'
          }
          loading={desktop.connecting}
          disabled={
            IS_IOS ||
            desktop.connecting ||
            desktop.updating ||
            (!(desktop.status.running && desktop.status.source === 'subscription') &&
              !state.canConnect)
          }
          on:click={() => {
            if (desktop.status.running && desktop.status.source === 'subscription')
              void disconnectProxy();
            else void connectSelected();
          }}
        >
          {desktop.status.running && desktop.status.source === 'subscription'
            ? '断开连接'
            : '连接节点'}
        </Button>
      </section>
      {(desktop.error || desktop.status.error) && (
        <div
          className="mt-4 rounded-lg border border-error/25 bg-error/5 px-4 py-3 text-xs wrap-break-word text-error"
          role="alert"
        >
          {desktop.error || desktop.status.error}
        </div>
      )}
      <section className="pt-5.5 pb-4.5">
        <div className="mb-2.5 flex flex-wrap items-center gap-x-3.5 gap-y-1 [&_h2]:text-sm [&_h2]:font-semibold [&>span]:text-[11px] [&>span]:text-secondary-text">
          <h2>订阅链接</h2>
          <SubscriptionShare url={state.url} />
        </div>
        <form
          className="flex flex-wrap items-center gap-2 [&>input]:h-10 [&>input]:min-w-0 [&>input]:flex-1 [&>input]:rounded-lg [&>input]:border [&>input]:border-border [&>input]:bg-transparent [&>input]:px-3 [&>input]:font-mono [&>input]:text-xs [&>input]:outline-none max-sm:[&>input]:basis-full [&>input:focus]:border-primary"
          on:submit={(event) => {
            event.preventDefault();
            void updateSubscription(state.url);
          }}
        >
          <input
            aria-label="订阅链接"
            type="text"
            value={state.url}
            autoComplete="off"
            placeholder="粘贴你的订阅链接 https://…"
            disabled={IS_IOS || desktop.updating}
            on:input={(event) => {
              state.url = event.target.value;
            }}
          />
          <Button
            type="primary"
            on:click={() => {
              void updateSubscription(state.url);
            }}
            loading={desktop.updating}
            disabled={
              IS_IOS ||
              !state.url.trim() ||
              desktop.connecting ||
              desktop.testing ||
              Boolean(desktop.testingNodeId)
            }
          >
            更新订阅
          </Button>
        </form>
        {desktop.updatedAt > 0 && (
          <div className="mt-2.5 text-[10px] text-secondary-text">
            {updatedText(desktop.updatedAt)}
          </div>
        )}
      </section>
      <section>
        <div className="mb-4.5 flex flex-wrap items-center justify-between gap-3 [&_h2]:flex [&_h2]:items-center [&_h2]:gap-2 [&_h2]:text-sm [&_h2]:font-semibold [&_p]:mt-1 [&_p]:text-[10px] [&_p]:text-secondary-text">
          <div>
            <h2>
              代理节点{' '}
              <span className="rounded bg-primary/5 px-1.5 py-px text-[10px] text-secondary-text">
                {desktop.nodes.length}
              </span>
            </h2>
            <p>{state.available} 个节点通过真连接测试（5 秒超时）</p>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              aria-label="自动切换节点"
              title={
                globalSettings.subscriptionAutoSwitch ? '关闭自动切换节点' : '开启自动切换节点'
              }
              aria-pressed={globalSettings.subscriptionAutoSwitch}
              disabled={IS_IOS || !desktop.nodes.length || desktop.updating}
              className={cx(
                'grid size-9 cursor-pointer place-items-center rounded-lg border bg-transparent text-lg disabled:opacity-40',
                globalSettings.subscriptionAutoSwitch
                  ? 'border-primary text-primary'
                  : 'border-border text-secondary-text hover:text-primary',
              )}
              on:click={toggleAutoSwitch}
            >
              <span className="icon-[ant-design--swap-outlined]" />
            </button>
            <button
              type="button"
              aria-label="测试延迟"
              title="测试延迟"
              aria-busy={desktop.testing}
              className="grid size-9 cursor-pointer place-items-center rounded-lg border border-border bg-transparent text-lg text-secondary-text hover:text-primary disabled:cursor-default disabled:opacity-40"
              disabled={
                !desktop.nodes.length ||
                desktop.testing ||
                Boolean(desktop.testingNodeId) ||
                desktop.updating ||
                desktop.connecting
              }
              on:click={() => {
                void testNodes();
              }}
            >
              <span
                className={cx(
                  desktop.testing
                    ? 'icon-[ant-design--loading-outlined] animate-spin'
                    : 'icon-[ant-design--dashboard-outlined]',
                )}
              />
            </button>
            <button
              type="button"
              aria-label="按延迟排序"
              title={state.sortByLatency ? '恢复订阅顺序' : '按延迟从低到高排序'}
              aria-pressed={state.sortByLatency}
              className={cx(
                'grid size-9 cursor-pointer place-items-center rounded-lg border bg-transparent text-lg disabled:cursor-default disabled:opacity-40',
                state.sortByLatency
                  ? 'border-primary text-primary'
                  : 'border-border text-secondary-text hover:text-primary',
              )}
              disabled={desktop.nodes.length < 2 || desktop.testing || desktop.updating}
              on:click={() => {
                state.sortByLatency = !state.sortByLatency;
              }}
            >
              <span className="icon-[ant-design--sort-ascending-outlined]" />
            </button>
          </div>
        </div>
        {!desktop.ready ? (
          <div className="flex min-h-52 flex-col items-center justify-center rounded-xl border border-dashed border-border px-4 text-center text-xs text-secondary-text [&_h3]:mb-2 [&_h3]:text-base [&_p]:text-xs [&>span]:mb-4 [&>span]:text-4xl">
            正在读取本地订阅…
          </div>
        ) : !desktop.nodes.length ? (
          <div className="flex min-h-52 flex-col items-center justify-center rounded-xl border border-dashed border-border px-4 text-center text-xs text-secondary-text [&_h3]:mb-2 [&_h3]:text-base [&_p]:text-xs [&>span]:mb-4 [&>span]:text-4xl">
            <span className="icon-[ant-design--global-outlined]" />
            <h3>从一个订阅开始</h3>
            <p>填入订阅链接并更新，即可在这里选择代理节点。</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-3 min-[430px]:grid-cols-[repeat(auto-fill,minmax(215px,1fr))]">
            {state.visible.map((node) => (
              <div key={node.id} className="relative min-w-0">
                <button
                  type="button"
                  className={cx(
                    'flex size-full min-w-0 cursor-pointer flex-col gap-3 rounded-xl border px-4 py-4 text-left text-normal-text transition-colors enabled:hover:border-primary/60 disabled:cursor-default disabled:opacity-55',
                    node.id === globalSettings.subscriptionSelectedId
                      ? 'border-primary bg-primary/5'
                      : 'border-border bg-transparent',
                    Boolean(node.unsupportedReason) && 'border-dashed',
                  )}
                  disabled={
                    desktop.connecting ||
                    desktop.updating ||
                    Boolean(node.unsupportedReason) ||
                    node.latencyMs === null ||
                    node.latencyMs >= 5000
                  }
                  aria-pressed={node.id === globalSettings.subscriptionSelectedId}
                  on:click={() => {
                    void selectNode(node);
                  }}
                >
                  <div className="flex items-center justify-between gap-2.5 text-[10px]">
                    <span className="rounded bg-primary/5 px-1.5 py-1 text-[9px] tracking-wide text-secondary-text">
                      {node.protocol.toUpperCase()}
                    </span>
                    <span
                      className={cx(
                        'text-[10px]',
                        node.latencyMs !== null && node.latencyMs < 5000
                          ? 'text-emerald-500'
                          : 'text-secondary-text',
                      )}
                    >
                      {node.unsupportedReason ? (
                        '暂不支持'
                      ) : (desktop.testing && !node.tested) || desktop.testingNodeId === node.id ? (
                        <span
                          role="status"
                          aria-label="测速中"
                          className="inline-flex items-center"
                        >
                          <span className="icon-[ant-design--loading-outlined] animate-spin" />
                        </span>
                      ) : (
                        latencyText(node.latencyMs, node.tested)
                      )}
                    </span>
                  </div>
                  <strong className="truncate text-[13px] font-semibold">{node.name}</strong>
                  <div
                    className={cx(
                      'flex min-w-0 gap-2 text-[10px] text-secondary-text [&>span:first-child]:flex-1 [&>span:first-child]:truncate',
                      globalSettings.subscriptionAutoSwitch && 'pr-7',
                    )}
                  >
                    <span>{node.unsupportedReason || `${node.address}:${node.port}`}</span>
                  </div>
                </button>
                {globalSettings.subscriptionAutoSwitch && (
                  <button
                    type="button"
                    aria-label={`候选节点：${node.name}`}
                    title={node.unsupportedReason || '自动切换候选节点'}
                    aria-pressed={globalSettings.subscriptionCandidates?.includes(node.id) ?? false}
                    disabled={Boolean(node.unsupportedReason) || desktop.updating}
                    className={cx(
                      'absolute right-2 bottom-2 grid size-7 cursor-pointer place-items-center rounded-md border border-transparent bg-transparent text-base hover:bg-primary/10 disabled:opacity-40',
                      globalSettings.subscriptionCandidates?.includes(node.id)
                        ? 'text-primary'
                        : 'text-secondary-text',
                    )}
                    on:click={() => toggleAutoSwitchCandidate(node.id)}
                  >
                    <span className="icon-[ant-design--swap-outlined]" />
                  </button>
                )}
              </div>
            ))}
          </div>
        )}
      </section>
    </div>
  );
}
