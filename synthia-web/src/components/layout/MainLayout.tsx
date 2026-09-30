import { Outlet } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { Header } from './Header';
import { Sidebar } from './Sidebar';
import { CommandPalette } from '../ui/CommandPalette';
import { useMediaQuery } from '../../hooks/useMediaQuery';
import { useSidebarCollapse } from '../../hooks/useSidebarCollapse';

export interface MainLayoutProps {
  isServerAvailable: boolean;
}

/**
 * Below this width the labelled sidebar cannot coexist with the
 * thread, so the rail is forced. The value mirrors the
 * `max-width: 767px` guard in `tokens.css` — the stylesheet keeps
 * a `max-width` clamp as a belt-and-braces guard, but this is what
 * decides the rail's *presentation* (labels, hints, tooltips).
 */
const NARROW_VIEWPORT_QUERY = '(max-width: 767px)';

/**
 * Primary application layout: header on top, sidebar on left,
 * routed page content in the main area.
 *
 * The sidebar's collapsed state is owned here because this is the
 * common ancestor of the rail and the main column: collapsing has
 * to reflow both, and keeping it out of the router means switching
 * pages (a new `<Outlet>` child) never resets it.
 *
 * `<CommandPalette />` is mounted here for the same reason: it is
 * shell chrome (it owns its own open state and its `Ctrl/Cmd+K`
 * binding), it must be reachable from every page, and as a child of
 * the layout route it stays mounted across navigations — so
 * selection state and in-flight fetches survive a route change the
 * palette itself triggered.
 */
export function MainLayout({ isServerAvailable }: MainLayoutProps) {
  const { collapsed, toggle } = useSidebarCollapse();
  const isNarrow = useMediaQuery(NARROW_VIEWPORT_QUERY);
  // On a narrow viewport the rail is not a choice, so the toggle is
  // withheld (`onToggle` undefined) rather than presented as a dead
  // control that flips state the user cannot see.
  return (
    <Box style={{ height: '100vh', background: 'var(--bg-primary)' }}>
      <CommandPalette />
      <Header isServerAvailable={isServerAvailable} />
      <Flex className="nt-app-shell-row" style={{ height: 'calc(100vh - var(--size-header))' }}>
        <Sidebar collapsed={collapsed || isNarrow} onToggle={isNarrow ? undefined : toggle} />
        <Box asChild className="nt-app-main" style={{ flex: 1, overflow: 'auto' }}>
          <main>
            <Outlet />
          </main>
        </Box>
      </Flex>
    </Box>
  );
}
