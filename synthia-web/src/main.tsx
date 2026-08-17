import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';
import { initA2AClient } from './api/a2a-stream';
import { initTheme } from './hooks/useTheme';
import '@radix-ui/themes/styles.css';
import './styles/fonts.css';
import './styles/tokens.css';
import './index.css';

// Apply the saved theme before React mounts so the first
// paint already reflects the user's preference. Skipping this
// would flash a light surface for users who picked dark.
initTheme();

const root = document.getElementById('root');
if (!root) throw new Error('Root element #root not found');

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// Initialize A2A client asynchronously after render
// This prevents blocking the UI if the backend is slow/unavailable
initA2AClient().catch((err: unknown) => {
  console.warn('A2A client initialization failed:', err);
});
