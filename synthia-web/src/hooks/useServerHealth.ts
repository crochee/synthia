import { useEffect, useState } from 'react';

/**
 * Polls the backend `/health` endpoint on a fixed interval
 * and returns whether the server is currently reachable.
 *
 * The A2A SDK uses HTTP/SSE rather than a persistent WebSocket,
 * so "connection" is best modeled as "is the server responding
 * to health checks right now". 30s is a reasonable cadence that
 * balances freshness against backend load.
 */
const HEALTH_URL = '/health';
const CHECK_INTERVAL_MS = 30_000;

export function useServerHealth(): boolean {
  const [isServerAvailable, setIsServerAvailable] = useState(false);

  useEffect(() => {
    let cancelled = false;

    const check = async () => {
      try {
        const res = await fetch(HEALTH_URL, { method: 'GET' });
        if (!cancelled) setIsServerAvailable(res.ok);
      } catch {
        if (!cancelled) setIsServerAvailable(false);
      }
    };

    void check();
    const interval = setInterval(check, CHECK_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  return isServerAvailable;
}
