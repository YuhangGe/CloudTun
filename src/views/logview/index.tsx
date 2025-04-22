import { logStore } from '@/store/log';
import { ref, watch } from 'jinge';

export function LogView() {
  const el = ref<HTMLDivElement>();

  watch(logStore.logs, () => {
    if (!el.value) return;
    el.value.scroll({ behavior: 'smooth', top: el.value.scrollHeight });
  });

  return (
    <div className='mt-3 mb-4 flex-1 overflow-y-auto' ref={el}>
      {logStore.logs.map((log) => (
        <p className='text-secondary-text mb-0.5 font-mono leading-[1.2]' key={log.id}>
          {log.text}
        </p>
      ))}
    </div>
  );
}
