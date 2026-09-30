import { NavLink } from 'react-router-dom';
import { Box, Flex, Text, Button } from '@radix-ui/themes';
import './Sidebar.css';

interface NavItem {
  path: string;
  label: string;
  /** Glyph shown in the collapsed rail. Radix has no icon set of
   *  its own and the project takes no icon dependency, so the rail
   *  uses the same emoji glyphs the rest of the UI already paints
   *  (🤖 agent chip, 📎 attach, ⚠️ / 🔍 empty states) rather than
   *  inventing a second visual vocabulary. */
  icon: string;
  shortcut: string;
}

const NAV_ITEMS: NavItem[] = [
  { path: '/chat', label: 'Chat', icon: '💬', shortcut: 'C' },
  { path: '/tools', label: 'Tools', icon: '🔧', shortcut: 'T' },
  { path: '/agents', label: 'Agents', icon: '🤖', shortcut: 'G' },
  { path: '/skills', label: 'Skills', icon: '📚', shortcut: 'K' },
  { path: '/sessions', label: 'Sessions', icon: '🗂️', shortcut: 'S' },
  { path: '/schedules', label: 'Schedules', icon: '⏰', shortcut: 'H' },
  { path: '/workflows', label: 'Workflows', icon: '🔀', shortcut: 'W' },
  { path: '/evals', label: 'Evals', icon: '📊', shortcut: 'E' },
  { path: '/tasks', label: 'Tasks', icon: '🧭', shortcut: 'D' },
];

export interface SidebarProps {
  /** Render the icon rail instead of the labelled column. */
  collapsed: boolean;
  /** Omitted when a control could not change anything (narrow
   *  viewport, where the rail is forced) — the toggle disappears
   *  rather than sitting there inert. */
  onToggle?: () => void;
}

/**
 * Side navigation using Radix Themes' Button + active NavLink styling.
 * The active indicator is a 3px left border applied via inline style.
 *
 * Each item exposes a `g+<shortcut>` accelerator via the
 * `aria-keyshortcuts` attribute so screen readers announce
 * the binding alongside the label. The visual `<kbd>` element
 * mirrors the same key for sighted users. Bindings are
 * implemented globally by `useKeyboardShortcuts`.
 *
 * Collapsed (rail) mode keeps every destination reachable: the
 * glyph stays, the label stays in the DOM for screen readers, and
 * a native `title` supplies the tooltip sighted users need to tell
 * the icons apart. The `g <key>` accelerators are global, so they
 * keep working in either state.
 */
export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  return (
    <Box asChild className={`nt-sidebar${collapsed ? ' nt-sidebar--collapsed' : ''}`}>
      <nav aria-label="Primary navigation" id="nt-sidebar-nav">
        <Flex direction="column" gap="1" p="3" className="nt-sidebar__body">
          <Flex align="center" justify="between" className="nt-sidebar__head">
            <Text size="1" weight="medium" color="gray" className="nt-sidebar__section">
              Navigation
            </Text>
            {onToggle && (
              <button
                type="button"
                className="nt-sidebar__toggle"
                onClick={onToggle}
                aria-expanded={!collapsed}
                aria-controls="nt-sidebar-nav"
                aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
                title={`${collapsed ? 'Expand' : 'Collapse'} sidebar (Ctrl+B)`}
                data-testid="sidebar-toggle"
              >
                <span aria-hidden>{collapsed ? '»' : '«'}</span>
              </button>
            )}
          </Flex>
          {NAV_ITEMS.map((item) => (
            <NavLink
              key={item.path}
              to={item.path}
              style={{ textDecoration: 'none' }}
              title={collapsed ? item.label : undefined}
              data-testid={`nav-${item.path.slice(1)}`}
            >
              {({ isActive }) => (
                <Button
                  variant={isActive ? 'solid' : 'ghost'}
                  color={isActive ? 'blue' : 'gray'}
                  size="2"
                  aria-keyshortcuts={`G ${item.shortcut}`}
                  aria-current={isActive ? 'page' : undefined}
                  style={{
                    width: '100%',
                    justifyContent: collapsed ? 'center' : 'flex-start',
                    borderLeft: isActive
                      ? '3px solid var(--accent-primary)'
                      : '3px solid transparent',
                  }}
                >
                  <Flex
                    align="center"
                    justify={collapsed ? 'center' : 'between'}
                    width="100%"
                    className="nt-sidebar__row"
                  >
                    <Flex align="center" gap="2" className="nt-sidebar__identity">
                      <span aria-hidden className="nt-sidebar__icon">
                        {item.icon}
                      </span>
                      <Text size="2" weight="medium" className="nt-sidebar__label">
                        {item.label}
                      </Text>
                    </Flex>
                    <Text size="1" color="gray" className="nt-sidebar__hint">
                      <kbd style={{ fontFamily: 'inherit', fontSize: 'inherit' }}>
                        g {item.shortcut}
                      </kbd>
                    </Text>
                  </Flex>
                </Button>
              )}
            </NavLink>
          ))}
        </Flex>
        <Box
          px="4"
          py="3"
          style={{ borderTop: '1px solid var(--border-subtle)' }}
          className="nt-sidebar__footer"
        >
          <Text size="1" color="gray" style={{ fontStyle: 'italic' }}>
            v0.1.0
          </Text>
        </Box>
      </nav>
    </Box>
  );
}
