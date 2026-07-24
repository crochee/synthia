import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { MainLayout } from './components/layout/MainLayout';
import { useServerHealth } from './hooks/useServerHealth';
import { ChatPage } from './pages/ChatPage';
import { ToolsPage } from './pages/ToolsPage';
import { SkillsPage } from './pages/SkillsPage';
import { SettingsPage } from './pages/SettingsPage';
import { TasksPage } from './pages/TasksPage';
import { MemoryPage } from './pages/MemoryPage';
import { JobsPage } from './pages/JobsPage';
import { McpPage } from './pages/McpPage';
import './styles/page.css';

/**
 * Top-level application component.
 *
 * Wires the React Router tree under the main layout
 * (header + sidebar + outlet) and feeds the layout the
 * live server-health status from `useServerHealth`.
 */
export default function App() {
  const isServerAvailable = useServerHealth();

  return (
    <BrowserRouter>
      <Routes>
        <Route element={<MainLayout isServerAvailable={isServerAvailable} />}>
          <Route path="/" element={<Navigate to="/chat" replace />} />
          <Route path="/chat" element={<ChatPage />} />
          <Route path="/chat/:sessionId" element={<ChatPage />} />
          <Route path="/tools" element={<ToolsPage />} />
          <Route path="/skills" element={<SkillsPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/tasks" element={<TasksPage />} />
          <Route path="/memory" element={<MemoryPage />} />
          <Route path="/jobs" element={<JobsPage />} />
          <Route path="/mcp" element={<McpPage />} />
          <Route path="*" element={<Navigate to="/chat" replace />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
