import { type JNode, type WithChildren, cx } from 'jinge';

export function FormItem(
  props: {
    className?: string;
    label: string;
    required?: boolean;
    error?: string;
  } & WithChildren<JNode>,
) {
  return (
    <div
      className={cx(
        'flex w-full flex-col gap-2 sm:flex-row sm:items-start sm:gap-3',
        props.className,
      )}
    >
      <label className="flex shrink-0 items-center pt-1 whitespace-nowrap sm:w-28">
        {props.required !== false && <span className="mt-0.75 mr-1 text-base text-red-500">*</span>}
        {props.label}
      </label>
      <div
        className={cx(
          'flex w-full min-w-0 flex-1 flex-col gap-1 [&>*:first-child]:min-h-8 [&>*:first-child]:max-w-full [&>*:first-child]:min-w-0 [&>*:first-child]:flex-1',
          props.error && '[&>*:first-child]:border-error',
        )}
      >
        {props.children}
        {props.error && <p className="text-xs text-error">{props.error}</p>}
      </div>
    </div>
  );
}
