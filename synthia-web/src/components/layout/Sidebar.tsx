import { NavLink } from 'react-router-dom';
import './Sidebar.css';

interface NavItem {
  path: string;
  label: string;
  shortcut: string;
}

const NAV_ITEMS: NavItem[] = [
  { path: '/chat', label: 'CHAT', shortcut: 'C' },
  { path: '/tools', label: 'TOOLS', shortcut: 'T' },
  { path: '/skills', label: 'SKILLS', shortcut: 'K' },
  { path: '/tasks', label: 'TASKS', shortcut: 'A' },
  { path: '/memory', label: 'MEMORY', shortcut: 'M' },
  { path: '/jobs', label: 'JOBS', shortcut: 'J' },
  { path: '/mcp', label: 'MCP', shortcut: 'X' },
  { path: '/settings', label: 'SETTINGS', shortcut: 'S' },
];

/**
 * Side navigation with terminal-style menu items.
 * Uses NavLink from react-router to highlight the active route.
 */
export function Sidebar() {
  return (
    <nav className="nt-sidebar" aria-label="Primary navigation">
      <div className="nt-sidebar__nav">
        {NAV_ITEMS.map((item) => (
          <NavLink
            key={item.path}
            to={item.path}
            className={({ isActive }: { isActive: boolean }) =>
              `nt-sidebar__item ${isActive ? 'is-active' : ''}`
            }
          >
            <span className="nt-sidebar__shortcut">[{item.shortcut}]</span>
            <span className="nt-sidebar__label">{item.label}</span>
          </NavLink>
        ))}
      </div>
      <div className="nt-sidebar__footer">
        <span className="nt-sidebar__version">v0.1.0</span>
      </div>
    </nav>
  );
}
