import { Card } from '../components/ui/Card';
import { useEffect, useState } from 'react';
import { api } from '../api/client';
import type { Skill } from '../api/types';

export function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .get<Skill[]>('/api/skills')
      .then((data) => {
        if (cancelled) return;
        setSkills(Array.isArray(data) ? data : []);
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
      <h1 className="nt-page-title">Skills</h1>
      {error && (
        <Card glow="red" title="Error">
          <code>{error}</code>
        </Card>
      )}
      {skills.length === 0 && !error && (
        <Card title="No skills">No skills are currently registered.</Card>
      )}
      {skills.map((skill) => (
        <Card key={skill.name} title={skill.name} glow={skill.enabled ? 'green' : 'none'}>
          {skill.description && <p>{skill.description}</p>}
        </Card>
      ))}
    </div>
  );
}
