import { Outlet } from 'react-router-dom';
import { Header } from './Header';
import { Sidebar } from './Sidebar';
import './MainLayout.css';

export interface MainLayoutProps {
  isServerAvailable: boolean;
}

/**
 * Primary application layout: header on top, sidebar on left,
 * routed page content in the main area. Used as the React Router
 * layout route parent.
 */
export function MainLayout({ isServerAvailable }: MainLayoutProps) {
  return (
    <div className="nt-layout">
      <Header isServerAvailable={isServerAvailable} />
      <div className="nt-layout__body">
        <Sidebar />
        <main className="nt-layout__main">
          <Outlet />
        </main>
      </div>
    </div>
  );
}
