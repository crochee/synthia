import { useEffect, useMemo } from 'react';
import { useLocation, Link } from 'react-router-dom';
import { Flex, Badge, Box } from '@radix-ui/themes';
import { ThemeToggle } from '../ui/ThemeToggle';
import { useTheme } from '../../hooks/useTheme';

export interface HeaderProps {
  isServerAvailable: boolean;
}

interface Crumb {
  label: string;
  to?: string;
}

/**
 * Resolve a path into human-readable breadcrumb segments. The
 * first segment is always the top-level page (Chat / Tools /
 * Agents / Skills / Tasks); trailing segments get shortened to
 * the first 8 characters of an id, which is enough to be
 * recognisable in a breadcrumb without flooding the header.
 *
 * The lookup table is intentionally hard-coded — the nav tree
 * is small (5 entries) and changing it should require touching
 * this file as well as `Sidebar.tsx`, which keeps the two
 * surfaces in sync as a code review signal.
 */
const PAGE_LABEL: Record<string, string> = {
  chat: 'Chat',
  tools: 'Tools',
  agents: 'Agents',
  skills: 'Skills',
  tasks: 'Tasks',
};

function crumbsForPath(pathname: string): Crumb[] {
  const segments = pathname.split('/').filter(Boolean);
  if (segments.length === 0) return [{ label: 'Chat', to: '/chat' }];
  const [head, ...rest] = segments;
  const headLabel = PAGE_LABEL[head] ?? head;
  const crumbs: Crumb[] = [{ label: headLabel, to: `/${head}` }];
  let acc = `/${head}`;
  rest.forEach((seg) => {
    acc += `/${seg}`;
    crumbs.push({ label: seg.length > 12 ? `${seg.slice(0, 8)}…` : seg, to: acc });
  });
  return crumbs;
}

/**
 * Top-of-page header. Wordmark + breadcrumb + connection-status
 * badge + theme toggle. Wrapped in a real `<header>` element
 * so `getByRole('banner')` and accessibility tools see it
 * correctly.
 */
export function Header({ isServerAvailable }: HeaderProps) {
  const location = useLocation();
  const crumbs = useMemo(() => crumbsForPath(location.pathname), [location.pathname]);
  // `resolvedAppearance` collapses `system` onto the OS choice so
  // the logo src always tracks what the user actually sees (rather
  // than the persisted preference).
  const { resolvedAppearance } = useTheme();
  const logoSrc = resolvedAppearance === 'dark' ? '/logo-inverse.svg' : '/logo.svg';

  // Mirror the current section into the document title so the
  // browser tab and any history search reflect where the user
  // is. We always set the title (rather than append) so a
  // stale section doesn't bleed across navigations.
  useEffect(() => {
    const section = crumbs[0]?.label ?? 'Synthia';
    document.title = `Synthia · ${section}`;
  }, [crumbs]);

  return (
    <Box
      asChild
      px="4"
      py="3"
      style={{
        background: 'var(--bg-elevated)',
        borderBottom: '1px solid var(--border-subtle)',
        height: 56,
      }}
    >
      <header>
        <Flex align="center" justify="between" gap="3">
          <Flex align="center" gap="3">
            <Link
              to="/chat"
              aria-label="Synthia home"
              style={{ display: 'inline-flex', alignItems: 'center', textDecoration: 'none' }}
            >
              <img
                src={logoSrc}
                alt="Synthia"
                height={32}
                style={{
                  display: 'block',
                  height: 32,
                  width: 'auto',
                }}
              />
            </Link>
            <Box
              aria-hidden
              style={{
                color: 'var(--text-muted)',
                fontSize: 12,
                fontStyle: 'italic',
              }}
            >
              agent.runtime
            </Box>
            <Breadcrumbs items={crumbs} />
          </Flex>
          <Flex align="center" gap="3">
            <ThemeToggle />
            <Badge
              color={isServerAvailable ? 'green' : 'red'}
              variant="soft"
              size="2"
              aria-live="polite"
              data-testid="server-status"
            >
              {isServerAvailable ? 'Online' : 'Offline'}
            </Badge>
          </Flex>
        </Flex>
      </header>
    </Box>
  );
}

function Breadcrumbs({ items }: { items: Crumb[] }) {
  if (items.length === 0) return null;
  return (
    <nav aria-label="Breadcrumb" style={{ minWidth: 0 }}>
      <ol
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 'var(--spacing-xs)',
          margin: 0,
          padding: 0,
          listStyle: 'none',
          color: 'var(--text-secondary)',
          fontSize: 'var(--fs-sm)',
          fontFamily: 'var(--font-mono)',
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
        }}
      >
        {items.map((crumb, i) => {
          const isLast = i === items.length - 1;
          return (
            <li
              key={crumb.to ?? crumb.label}
              style={{ display: 'inline-flex', alignItems: 'center', gap: 'var(--spacing-xs)' }}
            >
              {crumb.to && !isLast ? (
                <Link
                  to={crumb.to}
                  style={{
                    color: 'var(--text-secondary)',
                    textDecoration: 'none',
                  }}
                >
                  {crumb.label}
                </Link>
              ) : (
                <span
                  aria-current={isLast ? 'page' : undefined}
                  style={{
                    color: isLast ? 'var(--text-primary)' : 'var(--text-secondary)',
                    fontWeight: isLast ? 'var(--fw-medium)' : 'var(--fw-normal)',
                  }}
                >
                  {crumb.label}
                </span>
              )}
              {!isLast && (
                <span aria-hidden style={{ color: 'var(--text-muted)' }}>
                  ›
                </span>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}
