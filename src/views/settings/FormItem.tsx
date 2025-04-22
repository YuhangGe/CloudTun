import { type JNode, type WithChildren, cx } from "jinge";

export function FormItem(props: {
  className?: string;
  label: string;
  required?: boolean;
  error?: string;
} & WithChildren<JNode>) {
  return <div className={cx('flex gap-1 items-center', props.className)}>
    <label className='flex items-center whitespace-nowrap w-[96px]'>
      {props.required !== false && (
        <span className='text-base text-red-500 mr-1 mt-[3px]'>*</span>
      )}
      {props.label}
    </label>
    <div
      className={cx(
        'flex flex-1 flex-col gap-1 [&>*:first-child]:min-h-8 [&>*:first-child]:flex-1',
        props.error && '[&>*:first-child]:border-error',
      )}
    >
      {props.children}
      {props.error && <p className='text-error text-xs'>{props.error}</p>}
    </div>
  </div>
}