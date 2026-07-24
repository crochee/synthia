import { useWebSocket } from '../hooks/useWebSocket';

interface ConnectionStatusProps {
  wsUrl: string;
}

export function ConnectionStatus({ wsUrl }: ConnectionStatusProps) {
  const { isConnected, reconnectAttempt, error } = useWebSocket({
    url: wsUrl,
    onMessage: () => {},
  });

  return (
    <div className="connection-status">
      <span className={`status-dot ${isConnected ? 'connected' : 'disconnected'}`}></span>
      <span className="status-text">
        {isConnected
          ? 'Connected'
          : reconnectAttempt > 0
            ? `Reconnecting (${reconnectAttempt})...`
            : 'Connecting...'}
      </span>
      {error && <span className="status-error">{error}</span>}
    </div>
  );
}
