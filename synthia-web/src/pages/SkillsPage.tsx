import { Card } from '../components/ui/Card';
import { Button } from '../components/ui/Button';
import { useEffect, useState } from 'react';

interface Skill {
  name: string;
  description?: string;
  enabled: boolean;
}

export function SkillsPage() {
  const [skills, setSkills] = useState<Skill[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('/api/skills')
      .then((r) => (r.ok ? r.json() : []))
      .then((data) => setSkills(Array.isArray(data) ? data : (data.skills ?? [])))
      .catch((e) => setError(e.message));
  }, []);

  const toggle = async (skill: Skill) => {
    setSkills((prev) =>
      prev.map((s) => (s.name === skill.name ? { ...s, enabled: !s.enabled } : s)),
    );
    try {
      await fetch(`/api/skills/${encodeURIComponent(skill.name)}`, {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ enabled: !skill.enabled }),
      });
    } catch (e) {
      // Roll back on error
      setSkills((prev) =>
        prev.map((s) => (s.name === skill.name ? { ...s, enabled: skill.enabled } : s)),
      );
      setError((e as Error).message);
    }
  };

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
          <Button
            variant={skill.enabled ? 'primary' : 'secondary'}
            onClick={() => toggle(skill)}
            data-testid={`toggle-${skill.name}`}
          >
            {skill.enabled ? 'Enabled' : 'Disabled'}
          </Button>
        </Card>
      ))}
    </div>
  );
}
