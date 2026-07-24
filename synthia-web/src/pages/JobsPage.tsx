import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { Job, JobsListResponse } from '../api/types';

export function JobsPage() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .get<JobsListResponse>('/api/jobs')
      .then((data) => {
        if (cancelled) return;
        const pausedKeys = new Set(data.paused ?? []);
        const merged = (data.jobs ?? []).map((j) => ({
          ...j,
          paused: pausedKeys.has(j.key),
        }));
        setJobs(merged);
      })
      .catch((e: Error) => {
        if (cancelled) return;
        setError(e.message);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const toggle = async (job: Job) => {
    setJobs((prev) => prev.map((j) => (j.key === job.key ? { ...j, paused: !j.paused } : j)));
    try {
      await api.post(`/api/jobs/${encodeURIComponent(job.key)}/pause`);
    } catch (e) {
      setJobs((prev) => prev.map((j) => (j.key === job.key ? { ...j, paused: job.paused } : j)));
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
        <Card key={job.key} title={job.key} glow={!job.paused ? 'green' : 'none'}>
          <p>{job.description}</p>
          <code>trigger: {job.trigger_desc}</code>
          <div>
            <Button
              variant={!job.paused ? 'primary' : 'secondary'}
              onClick={() => toggle(job)}
              data-testid={`job-toggle-${job.key}`}
            >
              {!job.paused ? 'Enabled' : 'Disabled'}
            </Button>
          </div>
        </Card>
      ))}
    </div>
  );
}
