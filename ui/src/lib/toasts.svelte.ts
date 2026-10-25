export type ToastKind = 'info' | 'ok' | 'warn' | 'error';

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  kind: ToastKind;
  title: string;
  body?: string;
  action?: ToastAction;
}

export const toasts: Toast[] = $state([]);
let nextId = 1;

export function toast(
  kind: ToastKind,
  title: string,
  body?: string,
  ms = kind === 'error' ? 8000 : 4500,
  action?: ToastAction,
): void {
  const id = nextId++;
  toasts.push({ id, kind, title, body, action });
  if (toasts.length > 4) toasts.shift();
  setTimeout(() => dismiss(id), ms);
}

export function dismiss(id: number): void {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}
