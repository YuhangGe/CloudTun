export type LogTone = 'normal' | 'warning' | 'error';

export function logTone(text: string): LogTone {
  const level = /^\[([^\]]+)\]/.exec(text)?.[1].split('::').at(-1)?.toLowerCase();
  // Respect the producer's level even when the error chain contains "error".
  if (level === 'error') return 'error';
  if (level === 'warn' || level === 'warning') return 'warning';
  if (level === 'info' || level === 'debug') return 'normal';
  if (/error|失败|超时|失联/i.test(text)) return 'error';
  if (/warn|警告/i.test(text)) return 'warning';
  return 'normal';
}
