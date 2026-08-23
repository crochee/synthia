import { useEffect, useRef } from 'react';
import { useNavigate } from 'react-router-dom';

/**
 * Maps sidebar navigation items to keyboard shortcuts.
 *
 * The convention is "g + <letter>" (a Vim / GitHub-style
 * two-key prefix) so we never steal single keys from form
 * inputs. The first `g` starts a 1-second window during which
 * the next key is consumed as a shortcut; if no second key
 * arrives, the `g` is dropped and the window closes.
 *
 * This hook is global: it lives at the App level so shortcuts
 * work from any page. Form elements (`<input>`, `<textarea>`,
 * `<select>`, contenteditable) swallow the keystroke before it
 * reaches the window listener, so typing "go to chat" in the
 * chat textarea will not trigger navigation.
 */

interface ShortcutEntry {
  key: string; // case-insensitive
  path: string;
}

const SHORTCUTS: ReadonlyArray<ShortcutEntry> = [
  { key: 'c', path: '/chat' },
  { key: 't', path: '/tools' },
  { key: 'g', path: '/agents' },
  { key: 'k', path: '/skills' },
  { key: 's', path: '/sessions' },
];

const PREFIX_WINDOW_MS = 1_000;

/**
 * Install a global keyboard listener for navigation shortcuts.
 * Returns nothing — the hook is fire-and-forget at App mount.
 *
 * Implementation note: the `g` prefix is a small state machine
 * stored in a ref so the hook doesn't trigger a re-render each
 * time the user presses `g`.
 */
export function useKeyboardShortcuts(): void {
  const navigate = useNavigate();
  // Refs hold the timing state. A ref avoids the "stale
  // closure over a counter" trap and lets us reset cleanly on
  // each mount.
  const prefixExpiresAtRef = useRef<number>(0);

  useEffect(() => {
    function isEditableTarget(target: EventTarget | null): boolean {
      if (!target || !(target instanceof HTMLElement)) return false;
      const tag = target.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
      if (target.isContentEditable) return true;
      return false;
    }

    function handleKeyDown(event: KeyboardEvent): void {
      // Modifier-laden combos (Cmd/Ctrl/Alt + anything) are
      // reserved for the browser and OS. Skip.
      if (event.metaKey || event.ctrlKey || event.altKey) return;

      if (isEditableTarget(event.target)) return;

      const now = Date.now();
      const inPrefixWindow = now < prefixExpiresAtRef.current;

      // Reset window if it expired.
      if (!inPrefixWindow) {
        prefixExpiresAtRef.current = 0;
      }

      if (event.key === 'g' && !inPrefixWindow) {
        // Open a 1-second window for the next key.
        prefixExpiresAtRef.current = now + PREFIX_WINDOW_MS;
        return;
      }

      if (inPrefixWindow) {
        const entry = SHORTCUTS.find((s) => s.key === event.key.toLowerCase());
        if (entry) {
          event.preventDefault();
          navigate(entry.path);
        }
        // Either way, close the window — we only act once per
        // prefix to avoid `g g c` chaining into two nav jumps.
        prefixExpiresAtRef.current = 0;
      }
    }

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [navigate]);
}
