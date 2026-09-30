import { useEffect, useState } from 'react';

/**
 * Subscribe to a CSS media query from React.
 *
 * Only the sidebar breakpoint needs this today, but the reason it
 * exists at all is worth stating: the app shell has exactly one
 * "is the viewport narrow" question, and answering it in CSS while
 * the sidebar's rail state is JS would mean two sources of truth
 * for one visual state — the label-hiding rules would have to be
 * written twice, and a persisted "expanded" preference would fight
 * the breakpoint. Reading the same query the stylesheet uses keeps
 * them in lockstep.
 *
 * `change` listeners on a `MediaQueryList` are supported
 * everywhere the rest of the app already relies on (Safari 14+,
 * every Chromium), so there is no `addListener` fallback.
 */
export function useMediaQuery(query: string): boolean {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);

  useEffect(() => {
    const list = window.matchMedia(query);
    // Re-read on (re)subscribe: the query text is stable per call
    // site, but the first render may have happened before the
    // viewport settled (e.g. a restored window size).
    setMatches(list.matches);
    const onChange = (event: MediaQueryListEvent) => setMatches(event.matches);
    list.addEventListener('change', onChange);
    return () => list.removeEventListener('change', onChange);
  }, [query]);

  return matches;
}
