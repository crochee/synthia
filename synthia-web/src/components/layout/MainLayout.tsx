import { Outlet } from 'react-router-dom';
import { Box, Flex } from '@radix-ui/themes';
import { Header } from './Header';
import { Sidebar } from './Sidebar';

export interface MainLayoutProps {
  isServerAvailable: boolean;
}

/**
 * Primary application layout: header on top, sidebar on left,
 * routed page content in the main area.
 */
export function MainLayout({ isServerAvailable }: MainLayoutProps) {
  return (
    <Box style={{ height: '100vh', background: 'var(--bg-primary)' }}>
      <Header isServerAvailable={isServerAvailable} />
      <Flex className="nt-app-shell-row" style={{ height: 'calc(100vh - 56px)' }}>
        <Sidebar />
        <Box asChild className="nt-app-main" style={{ flex: 1, overflow: 'auto' }}>
          <main>
            <Outlet />
          </main>
        </Box>
      </Flex>
    </Box>
  );
}
