import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { ToastViewport } from '../components/ui/ToastViewport';
import { ToastContext, type Toast, type ToastContextValue } from './useToast';

const DEFAULT_DURATION_MS = 4_000;

/**
 * Provider for the in-app toast queue. Renders a fixed-position
 * stack at the top-right of the viewport via `<ToastViewport>`.
 * Toasts auto-dismiss after their `durationMs` (default 4s); a
 * duration of 0 keeps the toast until the user dismisses it.
 *
 * The queue is FIFO; new toasts append to the tail. Each toast
 * has an independent timer — pausing one toast does not affect
 * the others.
 *
 * Lives in a `.tsx` file (separate from `useToast.ts`) so the
 * `react-refresh/only-export-components` ESLint rule is happy
 * during HMR.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timersRef = useRef<Map<string, ReturnType<typeof setTimeout>>>(new Map());

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
    const timer = timersRef.current.get(id);
    if (timer) {
      clearTimeout(timer);
      timersRef.current.delete(id);
    }
  }, []);

  const push = useCallback<ToastContextValue['push']>(
    (toast) => {
      const id =
        typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function'
          ? crypto.randomUUID()
          : `toast-${Date.now()}-${Math.random().toString(36).slice(2)}`;
      const durationMs = toast.durationMs ?? DEFAULT_DURATION_MS;
      setToasts((prev) => [...prev, { ...toast, id, durationMs }]);
      if (durationMs > 0) {
        const timer = setTimeout(() => dismiss(id), durationMs);
        timersRef.current.set(id, timer);
      }
      return id;
    },
    [dismiss],
  );

  const clear = useCallback(() => {
    timersRef.current.forEach((t) => clearTimeout(t));
    timersRef.current.clear();
    setToasts([]);
  }, []);

  // Cleanup all timers on unmount so we don't leak.
  useEffect(() => {
    const timers = timersRef.current;
    return () => {
      timers.forEach((t) => clearTimeout(t));
      timers.clear();
    };
  }, []);

  const value = useMemo<ToastContextValue>(
    () => ({ toasts, push, dismiss, clear }),
    [toasts, push, dismiss, clear],
  );

  return (
    <ToastContext.Provider value={value}>
      {children}
      <ToastViewport toasts={toasts} onDismiss={dismiss} />
    </ToastContext.Provider>
  );
}
