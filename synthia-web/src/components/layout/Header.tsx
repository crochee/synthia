import './Header.css';

export interface HeaderProps {
  isServerAvailable: boolean;
}

/**
 * Top-of-page header. Shows the Synthia wordmark and a
 * connection-status indicator that pulses green when the
 * backend is reachable and red when it's not.
 */
export function Header({ isServerAvailable }: HeaderProps) {
  return (
    <header className="nt-header">
      <div className="nt-header__brand">
        <span className="nt-header__prompt">&gt;_</span>
        <span className="nt-header__title">SYNTHIA</span>
        <span className="nt-header__tagline">agent.runtime</span>
      </div>
      <div
        className={`nt-header__status ${isServerAvailable ? 'is-online' : 'is-offline'}`}
        role="status"
        aria-live="polite"
      >
        <span className="nt-header__status-dot" aria-hidden="true" />
        <span className="nt-header__status-label">{isServerAvailable ? 'ONLINE' : 'OFFLINE'}</span>
      </div>
    </header>
  );
}
