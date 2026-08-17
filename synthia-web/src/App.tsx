import { Suspense, lazy } from 'react';
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { Theme, Box } from '@radix-ui/themes';
import { MainLayout } from './components/layout/MainLayout';
import { ErrorBoundary } from './components/layout/ErrorBoundary';
import { useServerHealth } from './hooks/useServerHealth';
import { ToastProvider } from './hooks/useToast';
import { useKeyboardShortcuts } from './hooks/useKeyboardShortcuts';
import { useTheme } from './hooks/useTheme';
import { SkeletonList } from './components/ui/SkeletonList';
import './styles/page.css';

// Each page is loaded as a separate chunk so the initial
// download only includes ChatPage + MainLayout + AppShell.
// Subsequent navigations lazy-load the destination page on
// demand. ChatPage itself stays eager because it's the default
// route — there's no point delaying the first render behind
// a Suspense boundary.
const ToolsPage = lazy(() => import('./pages/ToolsPage').then((m) => ({ default: m.ToolsPage })));
const ToolDetailPage = lazy(() =>
  import('./pages/ToolDetailPage').then((m) => ({ default: m.ToolDetailPage })),
);
const SkillsPage = lazy(() =>
  import('./pages/SkillsPage').then((m) => ({ default: m.SkillsPage })),
);
const SkillDetailPage = lazy(() =>
  import('./pages/SkillDetailPage').then((m) => ({ default: m.SkillDetailPage })),
);
const TasksPage = lazy(() =>
  import('./pages/TasksPage').then((m) => ({ default: m.TasksPage })),
);
const TaskDetailPage = lazy(() =>
  import('./pages/TaskDetailPage').then((m) => ({ default: m.TaskDetailPage })),
);
const AgentsPage = lazy(() =>
  import('./pages/AgentsPage').then((m) => ({ default: m.AgentsPage })),
);
const AgentDetailPage = lazy(() =>
  import('./pages/AgentDetailPage').then((m) => ({ default: m.AgentDetailPage })),
);

// Eager ChatPage import — kept below the lazy declarations so
// the `lazy()` references above stay top-of-file for code-
// review visibility, but ES modules hoist `import` statements
// above any runtime code regardless of textual position.
import { ChatPage } from './pages/ChatPage';

/**
 * Top-level application component.
 *
 * Wires the React Router tree under the main layout
 * (header + sidebar + outlet) and feeds the layout the
 * live server-health status from `useServerHealth`.
 *
 * The router tree is wrapped in an `<ErrorBoundary>` so a
 * render-phase throw from any page surfaces as a recoverable
 * fallback instead of a blank screen. `<ToastProvider>` lives
 * inside the boundary so toasts themselves can never crash the
 * tree.
 */
export default function App() {
  const isServerAvailable = useServerHealth();
  const { resolvedAppearance } = useTheme();
  // `useKeyboardShortcuts` itself is wired inside
  // `<RouterShell>` (below) — it calls `useNavigate`, which
  // requires a `<Router>` ancestor. Calling it from `App()`
  // would throw on every render because `App()` is the
  // *parent* of `<BrowserRouter>`, not a descendant.

  return (
    <Theme appearance={resolvedAppearance} accentColor="blue" grayColor="slate" radius="medium" scaling="100%">
      <ErrorBoundary>
        <ToastProvider>
          <BrowserRouter>
            <RouterShell isServerAvailable={isServerAvailable} />
          </BrowserRouter>
        </ToastProvider>
      </ErrorBoundary>
    </Theme>
  );
}

/**
 * Lives inside `<BrowserRouter>` so hooks that depend on the
 * router context (`useNavigate`, `useLocation`, etc.) can run
 * during render. Today only the keyboard-shortcuts handler needs
 * to be inside the router; the rest of the tree (routes,
 * layout, outlet) is plain JSX. Splitting it out keeps the
 * `App` function itself router-agnostic while still letting
 * router-coupled hooks participate in its render path.
 */
function RouterShell({ isServerAvailable }: { isServerAvailable: boolean }) {
  useKeyboardShortcuts();
  return (
    <Routes>
              <Route element={<MainLayout isServerAvailable={isServerAvailable} />}>
                <Route path="/" element={<Navigate to="/chat" replace />} />
                <Route path="/chat" element={<ChatPage />} />
                <Route path="/chat/:sessionId" element={<ChatPage />} />
                <Route
                  path="/tools"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-tools" />}>
                      <ToolsPage />
                    </Suspense>
                  }
                />
                <Route
                  path="/tools/:name"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-tool-detail" />}>
                      <ToolDetailPage />
                    </Suspense>
                  }
                />
                <Route
                  path="/agents"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-agents" />}>
                      <AgentsPage />
                    </Suspense>
                  }
                />
                <Route
                  path="/agents/:name"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-agent-detail" />}>
                      <AgentDetailPage />
                    </Suspense>
                  }
                />
                <Route
                  path="/skills"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-skills" />}>
                      <SkillsPage />
                    </Suspense>
                  }
                />
                <Route
                  path="/skills/:name"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-skill-detail" />}>
                      <SkillDetailPage />
                    </Suspense>
                  }
                />
                <Route
                  path="/tasks"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-tasks" />}>
                      <TasksPage />
                    </Suspense>
                  }
                />
                <Route
                  path="/tasks/:id"
                  element={
                    <Suspense fallback={<RouteFallback testId="page-fallback-task-detail" />}>
                      <TaskDetailPage />
                    </Suspense>
                  }
                />
                <Route path="/sessions" element={<Navigate to="/tasks" replace />} />
                <Route path="*" element={<Navigate to="/chat" replace />} />
              </Route>
            </Routes>
  );
}

/**
 * Shown inside `<Suspense>` while a lazy chunk for a list /
 * detail page is in flight. Re-uses `<SkeletonList>` so the
 * fallback looks like the eventual content (a list of card
 * placeholders) — no surprise layout shift when the chunk
 * resolves.
 */
function RouteFallback({ testId }: { testId: string }) {
  return (
    <Box px="4" py="3" data-testid={testId}>
      <SkeletonList count={3} />
    </Box>
  );
}