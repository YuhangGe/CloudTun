import { logStore } from '@/store/log';
import { ref, watch } from 'jinge';

export function LogView() {
  const el = ref<HTMLDivElement>();

  watch(logStore.logs, () => {
    if (!el.value) return;
    el.value.scroll({ behavior: 'smooth', top: el.value.scrollHeight });
  });

  return (
    <div
      className='border-border mb-4 flex-1 overflow-auto rounded-lg py-3 max-sm:border max-sm:px-3'
      ref={el}
    >
      {logStore.logs.map((log) => (
        <p className='text-secondary-text mb-2 font-mono leading-[1.2]' key={log.id}>
          {log.text}
        </p>
      ))}
    </div>
  );
}
