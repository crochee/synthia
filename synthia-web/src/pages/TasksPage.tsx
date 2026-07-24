import { Card } from '../components/ui/Card';
import { useEffect, useState } from 'react';

interface TaskSummary {
  id: string;
  status: string;
  createdAt?: string;
  contextId?: string;
}

export function TasksPage() {
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/tasks')
      .then((r) => (r.ok ? r.json() : []))
      .then((data) => setTasks(Array.isArray(data) ? data : (data.tasks ?? [])))
      .catch((e) => setError(e.message));
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
