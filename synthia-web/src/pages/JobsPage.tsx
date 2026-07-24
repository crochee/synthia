import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { useEffect, useState } from 'react';

interface Job {
  id: string;
  name: string;
  schedule?: string;
  enabled: boolean;
}

export function JobsPage() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/jobs')
      .then((r) => (r.ok ? r.json() : []))
      .then((data) => setJobs(Array.isArray(data) ? data : (data.jobs ?? [])))
      .catch((e) => setError(e.message));
  }, []);

  const toggle = async (job: Job) => {
    setJobs((prev) => prev.map((j) => (j.id === job.id ? { ...j, enabled: !j.enabled } : j)));
    try {
      await fetch(`/api/jobs/${encodeURIComponent(job.id)}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: !job.enabled }),
      });
    } catch (e) {
      setJobs((prev) => prev.map((j) => (j.id === job.id ? { ...j, enabled: job.enabled } : j)));
      setError((e as Error).message);
    }
  };

  return (
    <div>
      <h1 className="nt-page-title">Jobs</h1>
      {error && (
        <Card glow="red" title="Error">
          <code>{error}</code>
        </Card>
      )}
      {jobs.length === 0 && !error && <Card title="No jobs">No scheduled jobs configured.</Card>}
      {jobs.map((job) => (
        <Card key={job.id} title={job.name} glow={job.enabled ? 'green' : 'none'}>
          {job.schedule && <code>schedule: {job.schedule}</code>}
          <div>
            <Button
              variant={job.enabled ? 'primary' : 'secondary'}
              onClick={() => toggle(job)}
              data-testid={`job-toggle-${job.id}`}
            >
              {job.enabled ? 'Enabled' : 'Disabled'}
            </Button>
          </div>
        </Card>
      ))}
    </div>
  );
}
