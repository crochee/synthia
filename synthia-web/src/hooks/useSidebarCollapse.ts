import { useCallback, useEffect, useState } from 'react';

import { isEditableTarget } from './useKeyboardShortcuts';

/** Storage key for the persisted rail preference. Shares the
 *  `synthia.` prefix the chat page uses for its drafts so a
 *  future "reset local state" sweep can find every key by
 *  prefix instead of keeping a hardcoded list. */
const STORAGE_KEY = 'synthia.sidebarCollapsed';

export interface SidebarCollapse {
  collapsed: boolean;
  toggle: () => void;
}

/**
 * Own the sidebar's collapsed/expanded state, persist it, and
 * bind `Ctrl/Cmd+B` (the Qoder desktop chord).
 *
 * The state lives in `MainLayout`, which is the common ancestor of
 * the sidebar and the main column, so both re-render from one
 * source of truth and a route change (which only swaps the
 * `<Outlet>`) never resets it. A first visit starts expanded —
 * an unexplained icon rail is a worse first impression than a
 * labelled one — and `localStorage` failures (Safari private
 * mode) fall back to that default rather than breaking the shell.
 *
 * About the chord: `useKeyboardShortcuts` deliberately ignores
 * every modified keystroke so `g <key>` navigation can never fire
 * from a browser/OS chord. This binding is the mirror image — it
 * fires *only* with `meta`/`ctrl` held, and only when no other
 * modifier is (so `Ctrl+Shift+B`, reserved by the desktop app for
 * its right-hand panel, is left alone). The shared
 * `isEditableTarget` guard keeps it out of text fields: pressing
 * `Ctrl+B` to jump the caret backwards in a textarea must keep
 * working.
 */
export function useSidebarCollapse(): SidebarCollapse {
  const [collapsed, setCollapsed] = useState(() => {
    try {
      return localStorage.getItem(STORAGE_KEY) === 'true';
    } catch {
      return false;
    }
  });

  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY, String(collapsed));
    } catch {
      // Storage unavailable — the in-memory state still works for
      // this page load, which is all the user asked for.
    }
  }, [collapsed]);

  const toggle = useCallback(() => setCollapsed((value) => !value), []);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent): void {
      if (!event.metaKey && !event.ctrlKey) return;
      if (event.altKey || event.shiftKey) return;
      if (event.key.toLowerCase() !== 'b') return;
      if (isEditableTarget(event.target)) return;
      event.preventDefault();
      toggle();
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [toggle]);

  return { collapsed, toggle };
}
