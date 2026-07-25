import { Card } from '../components/ui/Card';
import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { TaskSummary } from '../api/types';

export function TasksPage() {
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .get<{ tasks: TaskSummary[]; count: number }>('/api/tasks')
      .then((envelope) => {
        if (cancelled) return;
        // backend serialises `TaskListResponse { tasks: Vec<_>, count }` inside
        // the api.client envelope unwrap, so the value here is `{tasks, count}`
        // — not a bare array. Defensive fallback keeps the page rendering
        // even if the server temporarily downgrades.
        const list = Array.isArray(envelope?.tasks) ? envelope.tasks : [];
        setTasks(list);
      })
      .catch((e: Error) => {
        if (cancelled) return;
        setError(e.message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div>
      <h1 className="nt-page-title">Tasks</h1>
      {error && (
        <Card glow="red" title="Error">
          <code>{error}</code>
        </Card>
      )}
      {tasks.length === 0 && !error && <Card title="No tasks">No A2A tasks recorded yet.</Card>}
      {tasks.map((task) => (
        <Card
          key={task.id}
          title={`Task ${task.id.slice(0, 8)}`}
          glow={task.status === 'completed' ? 'green' : 'cyan'}
        >
          <div>
            <code>status: {task.status}</code>
          </div>
          {task.contextId && (
            <div>
              <code>context: {task.contextId}</code>
            </div>
          )}
          {task.createdAt && (
            <div>
              <code>created: {task.createdAt}</code>
            </div>
          )}
        </Card>
      ))}
    </div>
  );
}
