import { createContext, useContext } from 'react';
export { ToastProvider } from './useToast-provider';

type ToastVariant = 'info' | 'success' | 'warning' | 'error';

export interface Toast {
  id: string;
  variant: ToastVariant;
  message: string;
  /** Optional action button label + handler. */
  action?: { label: string; onClick: () => void };
  /** Auto-dismiss delay in ms. Use 0 to require manual dismissal. */
  durationMs: number;
}

export interface ToastContextValue {
  toasts: Toast[];
  push: (toast: Omit<Toast, 'id' | 'durationMs'> & { durationMs?: number }) => string;
  dismiss: (id: string) => void;
  clear: () => void;
}

/**
 * Module-level context. Lives in a `.ts` file (no JSX) so it
 * satisfies the `react-refresh/only-export-components` rule —
 * the provider component is in `useToast.tsx` next door.
 */
export const ToastContext = createContext<ToastContextValue | null>(null);

/**
 * Hook to push toasts from any component. Must be used inside a
 * `<ToastProvider>`. Throws when called outside the provider so
 * developers notice the missing wiring during development.
 */
export function useToast(): ToastContextValue {
  const ctx = useContext(ToastContext);
  if (!ctx) {
    throw new Error('useToast must be used inside <ToastProvider>');
  }
  return ctx;
}