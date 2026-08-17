import { useEffect, useState } from 'react';

/**
 * Reactive view of "is the Synthia backend reachable right now".
 *
 * Combines three signals:
 *   1. Periodic `/health` probe (30s cadence, skipped in
 *      background tabs) — the authoritative source for "the
 *      process is alive".
 *   2. `navigator.onLine` — the OS network indicator flips the
 *      indicator to OFFLINE immediately on disconnect without
 *      waiting for the next probe.
 *   3. A module-level store that the A2A stream layer can poke
 *      when it sees a network-level failure — this short-circuits
 *      the 30s polling window so the UI reacts to the same
 *      failure that broke the user's chat in flight.
 *
 * Because the three signals feed one module-level store
 * (`globalThis.__synthiaHealthStore`), every hook subscriber
 * sees the same value. `useSyncExternalStore` would be the
 * canonical choice here, but it forces a hard dependency on
 * React 18 — we keep `useState + subscribe` to avoid adding
 * another constraint on top of the existing 18.3 setup.
 */

const HEALTH_URL = '/health';
const CHECK_INTERVAL_MS = 30_000;

interface HealthStore {
  value: boolean;
  listeners: Set<(v: boolean) => void>;
}

type GlobalWithStore = typeof globalThis & {
  __synthiaHealthStore?: HealthStore;
};

function getStore(): HealthStore {
  const g = globalThis as GlobalWithStore;
  if (!g.__synthiaHealthStore) {
    g.__synthiaHealthStore = {
      // Default to false so the header briefly shows OFFLINE
      // before the first probe lands. Tests use the aria-live
      // region in the Header to wait for the ONLINE transition.
      value: false,
      listeners: new Set(),
    };
  }
  return g.__synthiaHealthStore;
}

function setValue(v: boolean): void {
  const store = getStore();
  if (store.value === v) return;
  store.value = v;
  store.listeners.forEach((cb) => cb(v));
}

/**
 * Set the health flag imperatively from outside the hook. Used
 * by the A2A stream layer so a fetch failure on a chat round
 * trip flips the indicator to OFFLINE without waiting for the
 * next 30s tick.
 *
 * The periodic probe will recover the value as soon as the
 * server comes back, regardless of who last called this.
 */
export function setServerHealth(available: boolean): void {
  setValue(available);
}

export function useServerHealth(): boolean {
  const [value, setLocal] = useState<boolean>(() => getStore().value);

  useEffect(() => {
    const store = getStore();
    const cb = (v: boolean) => setLocal(v);
    store.listeners.add(cb);
    return () => {
      store.listeners.delete(cb);
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    // Most-recent in-flight controller. Tracked so the cleanup
    // can abort any probe that's still resolving when the
    // component unmounts — otherwise the fetch is left dangling,
    // burning a network socket and surfacing an uncaught
    // promise rejection in the browser console.
    let inFlight: AbortController | null = null;

    const probe = async () => {
      if (document.visibilityState === 'hidden') return;
      // Abort the previous in-flight probe (if any) so a fast
      // second probe never piles up on top of a slow first one.
      if (inFlight) inFlight.abort();
      const controller = new AbortController();
      inFlight = controller;
      try {
        const res = await fetch(HEALTH_URL, {
          method: 'GET',
          signal: controller.signal,
        });
        if (!cancelled) setValue(res.ok);
      } catch {
        // AbortError fires when the effect is torn down before
        // the request resolves — that's expected, so don't
        // flip the indicator to OFFLINE in that case.
        if (!cancelled && controller.signal.aborted !== true) {
          setValue(false);
        }
      } finally {
        // Only clear the slot if we still own it; another
        // probe may have already replaced the controller.
        if (inFlight === controller) inFlight = null;
      }
    };

    void probe();
    const interval = setInterval(probe, CHECK_INTERVAL_MS);

    // navigator.onLine gives us an instantaneous OFFLINE flip
    // on OS-level disconnect (airplane mode, VPN drop, etc.)
    // without waiting for the probe to fail.
    const handleOffline = () => setValue(false);
    const handleOnline = () => {
      // Don't optimistically set true — kick a probe and let
      // it decide. navigator.onLine can be misleading (a captive
      // portal reports `online: true` but no upstream).
      void probe();
    };
    window.addEventListener('online', handleOnline);
    window.addEventListener('offline', handleOffline);

    return () => {
      cancelled = true;
      clearInterval(interval);
      window.removeEventListener('online', handleOnline);
      window.removeEventListener('offline', handleOffline);
      // Abort any probe still resolving when the effect is
      // torn down. The `cancelled` flag already prevents the
      // post-await `setValue` from firing; the abort is here
      // to free the socket and avoid an unhandled rejection
      // surfacing in the console.
      if (inFlight) inFlight.abort();
    };
  }, []);

  return value;
}