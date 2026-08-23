import { NavLink } from 'react-router-dom';
import { Box, Flex, Text, Button } from '@radix-ui/themes';

interface NavItem {
  path: string;
  label: string;
  shortcut: string;
}

const NAV_ITEMS: NavItem[] = [
  { path: '/chat', label: 'Chat', shortcut: 'C' },
  { path: '/tools', label: 'Tools', shortcut: 'T' },
  { path: '/agents', label: 'Agents', shortcut: 'G' },
  { path: '/skills', label: 'Skills', shortcut: 'K' },
  { path: '/sessions', label: 'Sessions', shortcut: 'S' },
];

/**
 * Side navigation using Radix Themes' Button + active NavLink styling.
 * The active indicator is a 3px left border applied via inline style.
 *
 * Each item exposes a `g+<shortcut>` accelerator via the
 * `aria-keyshortcuts` attribute so screen readers announce
 * the binding alongside the label. The visual `<kbd>` element
 * mirrors the same key for sighted users. Bindings are
 * implemented globally by `useKeyboardShortcuts`.
 */
export function Sidebar() {
  return (
    <Box
      asChild
      className="nt-sidebar"
      style={{
        width: 'var(--nt-sidebar-width, 220px)',
        background: 'var(--bg-secondary)',
        borderRight: '1px solid var(--border-subtle)',
        flexShrink: 0,
      }}
    >
      <nav aria-label="Primary navigation">
        <Flex direction="column" gap="1" p="3">
          {NAV_ITEMS.map((item) => (
            <NavLink key={item.path} to={item.path} style={{ textDecoration: 'none' }}>
              {({ isActive }) => (
                <Button
                  variant={isActive ? 'solid' : 'ghost'}
                  color={isActive ? 'blue' : 'gray'}
                  size="2"
                  aria-keyshortcuts={`G ${item.shortcut}`}
                  aria-current={isActive ? 'page' : undefined}
                  style={{
                    width: '100%',
                    justifyContent: 'flex-start',
                    borderLeft: isActive
                      ? '3px solid var(--accent-primary)'
                      : '3px solid transparent',
                  }}
                >
                  <Flex align="center" justify="between" width="100%" className="nt-sidebar__row">
                    <Text size="2" weight="medium" className="nt-sidebar__label">
                      {item.label}
                    </Text>
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
