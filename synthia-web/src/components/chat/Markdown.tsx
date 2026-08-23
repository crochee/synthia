import { lazy, Suspense, memo, useEffect, useState } from 'react';
import type * as React from 'react';

/**
 * Lazy-loaded markdown renderer. The full `react-markdown`
 * toolchain (remark-parse, remark-gfm, rehype-highlight,
 * highlight.js, and `highlight.js/styles/atom-one-light.css`)
 * is non-trivial — well over 200 kB on disk, ~50 kB gzipped —
 * and only the Chat / Skill / Agent detail pages actually need
 * it. Keeping it behind a dynamic import lets the rest of the
 * app boot without paying for the markdown parser.
 *
 * The first render returns a tiny skeleton with the raw
 * `<pre>` source visible, so the surrounding layout doesn't
 * pop in. Once the chunk resolves, the markdown replaces the
 * placeholder and `React.memo` ensures subsequent re-renders
 * with the same `source` don't re-parse the tree.
 */

// Lazy import the entire markdown pipeline so the parser
// + highlighter + CSS all sit in a single chunk that's only
// fetched on first markdown render.
const MarkdownInner = lazy(async () => {
  const [ReactMarkdown, remarkGfm, rehypeHighlight] = await Promise.all([
    import('react-markdown'),
    import('remark-gfm'),
    import('rehype-highlight'),
  ]);
  // Side-effect import: highlight.js's bundled theme CSS
  // (~10 kB). Importing inside the lazy chunk keeps it out
  // of the main bundle.
  await import('highlight.js/styles/atom-one-light.css');

  // Hoisted to module scope of the lazy chunk so the React
  // `memo` shallow-comparison on `Inner` actually skips work
  // when `source` is unchanged. The previous version
  // constructed fresh `remarkPlugins` / `rehypePlugins` /
  // `components` object literals on every render — the `memo`
  // bailing out (because those references differed) meant
  // markdown was re-parsed on every parent re-render, even
  // when the source string was identical. Long streaming
  // assistant replies (50+ chunks) re-rendered the markdown
  // wrapper that many times for nothing.
  const remarkPlugins = [remarkGfm.default];
  const rehypePlugins = [rehypeHighlight.default];
  const components = {
    a: ({
      node: _node,
      ...props
    }: { node?: unknown } & React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
      <a {...props} target="_blank" rel="noreferrer" />
    ),
  };

  const Inner = memo(function Inner({ source }: { source: string }) {
    return (
      <div className="nt-chat__md">
        <ReactMarkdown.default
          remarkPlugins={remarkPlugins}
          rehypePlugins={rehypePlugins}
          components={components}
        >
          {source}
        </ReactMarkdown.default>
      </div>
    );
  });
  return { default: Inner };
});

/**
 * Public component. Renders the lazy-loaded `<MarkdownInner>`
 * inside a `<Suspense>` boundary; while the chunk is loading
 * we show a lightweight fallback that preserves layout
 * (whitespace sized to the source's line count) so the page
 * doesn't jump when the chunk lands.
 */
export function Markdown({ source }: { source: string }) {
  return (
    <Suspense fallback={<MarkdownSkeleton source={source} />}>
      <MarkdownInner source={source} />
    </Suspense>
  );
}

/**
 * Cheap placeholder while the markdown chunk is in flight. We
 * count newlines and render a row of shimmering bars so the
 * height of the eventual render is in the right ballpark —
 * avoids a visible "layout pop" once the chunk resolves.
 */
function MarkdownSkeleton({ source }: { source: string }) {
  const lines = source.split('\n').length;
  // Cap the visual so a 5,000-line transcript doesn't draw
  // a 5,000-row skeleton. Most chat replies are < 40 lines,
  // and capping at 40 keeps the placeholder honest.
  const visible = Math.min(lines, 40);
  // Tiny RAF-driven shimmer so the placeholder reads as
  // "loading" instead of "stuck text".
  const [shimmer, setShimmer] = useState(true);
  useEffect(() => {
    if (!shimmer) return;
    const id = requestAnimationFrame(() => setShimmer(false));
    return () => cancelAnimationFrame(id);
  }, [shimmer]);
  return (
    <div
      aria-busy="true"
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 4,
        fontFamily: 'var(--font-mono)',
        opacity: shimmer ? 0.6 : 0.85,
        // `var(--transition-fast)` is the project's token for
        // "short interaction feedback" (150ms). Using the
        // literal `200ms` here would diverge from every other
        // component's hover / focus transition duration — and
        // would also defeat the prefers-reduced-motion override
        // we wired into tokens.css.
        transition: 'opacity var(--transition-fast)',
      }}
    >
      {Array.from({ length: visible }, (_, i) => (
        <span
          key={i}
          style={{
            display: 'block',
            height: 12,
            width: `${70 + ((i * 13) % 30)}%`,
            background: 'var(--bg-tertiary)',
            borderRadius: 3,
          }}
        />
      ))}
      {lines > 40 && (
        <span style={{ fontSize: 'var(--fs-sm)', color: 'var(--text-muted)' }}>
          +{lines - 40} more lines…
        </span>
      )}
    </div>
  );
}

// Re-export the ReactNode type so consumers don't need to
// re-import from react when they type `MarkdownProps`.
