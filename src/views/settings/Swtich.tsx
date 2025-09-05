import { type WithEvents, cx, vm } from 'jinge';

export function Switch(
  props: {
    value?: boolean;
  } & WithEvents<{
    change?: (value: boolean) => void;
  }>,
) {
  const state = vm({
    checked: !!props.value,
  });

  return (
    <div
      on:click={() => {
        state.checked = !state.checked;
        props['on:change']?.(state.checked);
      }}
      className={cx(
        'relative flex h-[22px] w-[44px] cursor-pointer items-center rounded-full transition-[background] select-none',
        state.checked ? 'bg-primary' : 'bg-black/25',
      )}
    >
      <div
        className='absolute h-[18px] w-[18px] rounded-full bg-white transition-[left]'
        style={`left: ${state.checked ? 'calc(100% - 3px - 18px)' : '3px'}`}
      ></div>
    </div>
  );
}
