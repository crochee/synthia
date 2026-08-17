import { useCallback, useEffect, useState } from 'react';

export type Theme = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'synthia.theme';

type GlobalWithTheme = typeof globalThis & {
  __synthiaThemeStore?: {
    value: Theme;
    listeners: Set<(t: Theme) => void>;
  };
};

function getStore() {
  const g = globalThis as GlobalWithTheme;
  if (!g.__synthiaThemeStore) {
    // Resolve the initial value lazily from localStorage so
    // an HMR reload picks up the user's previous choice.
    const stored = readStoredTheme();
    g.__synthiaThemeStore = {
      value: stored,
      listeners: new Set(),
    };
  }
  return g.__synthiaThemeStore;
}

function readStoredTheme(): Theme {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === 'light' || raw === 'dark' || raw === 'system') return raw;
  } catch {
    // localStorage unavailable — fall through to default.
  }
  return 'light';
}

function applyTheme(theme: Theme): void {
  if (typeof document === 'undefined') return;
  const root = document.documentElement;
  if (theme === 'system') {
    root.removeAttribute('data-theme');
    // The CSS cascade uses `data-theme` selectors; removing the
    // attribute lets the OS-level `prefers-color-scheme` take
    // over via the `@media` query in `tokens.css` (next addition).
  } else {
    root.setAttribute('data-theme', theme);
  }
}

/**
 * Resolve the user's stored preference to the *effective*
 * `light` / `dark` setting that Radix `<Theme appearance=...>`
 * understands. The `system` mode is mapped to whichever the OS
 * reports via `prefers-color-scheme`, with `light` as the safe
 * default when no media query is available.
 */
export type ResolvedAppearance = 'light' | 'dark';

function resolveAppearance(theme: Theme): ResolvedAppearance {
  if (theme === 'dark') return 'dark';
  if (theme === 'light') return 'light';
  // theme === 'system'
  if (typeof window !== 'undefined' && window.matchMedia) {
    return window.matchMedia('(prefers-color-scheme: dark)').matches
      ? 'dark'
      : 'light';
  }
  return 'light';
}

/**
 * Apply the initial theme synchronously before React mounts so
 * the first paint isn't a flash of light when the user picked
 * dark. Must be called once at module evaluation time
 * (top-level of `main.tsx`); reads from localStorage and
 * stamps the `data-theme` attribute on `<html>` immediately.
 *
 * No-op if `document` is unavailable (e.g. SSR / test env).
 */
export function initTheme(): void {
  if (typeof document === 'undefined') return;
  applyTheme(readStoredTheme());
}

function setStoreValue(theme: Theme): void {
  const store = getStore();
  if (store.value === theme) return;
  store.value = theme;
  store.listeners.forEach((cb) => cb(theme));
  applyTheme(theme);
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // best-effort persistence
  }
}

/**
 * Read and update the user's theme preference. The hook
 * participates in the same module-level store as `initTheme`,
 * so multiple components see the same value without prop
 * drilling.
 */
export function useTheme(): {
  theme: Theme;
  setTheme: (t: Theme) => void;
  resolvedAppearance: ResolvedAppearance;
} {
  const [theme, setLocal] = useState<Theme>(() => getStore().value);
  // Bumped on every OS `prefers-color-scheme` change so the
  // consuming component (App) re-renders and re-derives
  // `resolvedAppearance` even when the user's chosen `theme`
  // hasn't moved. The previous `setLocal((cur) => cur)` trick
  // bailed out because `theme === 'system'` was unchanged,
  // so App never re-rendered and Radix `<Theme appearance=...>`
  // stayed stale on macOS appearance flips. A dedicated
  // counter guarantees a new state value each tick.
  const [osVersion, bumpOsVersion] = useState(0);

  useEffect(() => {
    const store = getStore();
    const cb = (t: Theme) => setLocal(t);
    store.listeners.add(cb);
    return () => {
      store.listeners.delete(cb);
    };
  }, []);

  // The `system` choice needs to track OS changes live so a
  // user flipping their macOS appearance sees the UI react.
  // Bump `osVersion` to force a re-render of the consuming
  // component (App) so it re-derives `resolvedAppearance`
  // and feeds it to Radix `<Theme appearance=...>`. The CSS
  // layer also reacts because `applyTheme` removed
  // `data-theme` for `system`, so the cascade falls through
  // to the `prefers-color-scheme` media query in `tokens.css`.
  useEffect(() => {
    if (theme !== 'system') return;
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const onChange = () => {
      bumpOsVersion((v) => v + 1);
    };
    mq.addEventListener('change', onChange);
    return () => mq.removeEventListener('change', onChange);
  }, [theme]);

  const setTheme = useCallback((t: Theme) => setStoreValue(t), []);
  // `osVersion` is read on every render so React tracks it
  // as a dependency of the render path — bumping it (from
  // the `prefers-color-scheme` listener above) forces a new
  // `resolvedAppearance` even when the user's chosen `theme`
  // hasn't moved. We discard the read value (`osVersion -
  // osVersion` is `0`) so the resolver only depends on
  // `theme`, and explicitly destructure to avoid an unused-
  // variable lint warning.
  void osVersion;
  const resolvedAppearance = resolveAppearance(theme);
  return { theme, setTheme, resolvedAppearance };
}