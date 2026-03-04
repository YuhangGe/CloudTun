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
        'relative flex h-5.5 w-11 cursor-pointer items-center rounded-full transition-[background] select-none',
        state.checked ? 'bg-primary' : 'bg-black/25',
      )}
    >
      <div
        className="absolute h-4.5 w-4.5 rounded-full bg-white transition-[left]"
        style={`left: ${state.checked ? 'calc(100% - 3px - 18px)' : '3px'}`}
      ></div>
    </div>
  );
}
