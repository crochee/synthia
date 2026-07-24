import { useEffect, useState } from 'react';
import { Card } from '../components/ui/Card';
import { Input } from '../components/ui/Input';
import { Button } from '../components/ui/Button';

interface Settings {
  provider?: string;
  model?: string;
  apiKey?: string;
}

export function SettingsPage() {
  const [settings, setSettings] = useState<Settings>({});
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch('/api/settings')
      .then((res) => (res.ok ? res.json() : null))
      .then((body: { data?: Settings } | null) => {
        if (cancelled || !body?.data) return;
        setSettings(body.data);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, []);

  const save = async () => {
    setSaved(false);
    setError(null);
    try {
      const res = await fetch('/api/settings', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings),
      });
      if (res.ok) setSaved(true);
      else setError(`Save failed: HTTP ${res.status}`);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  return (
    <div>
      <h1 className="nt-page-title">Settings</h1>
      {error && (
        <Card glow="red" title="Error">
          <code>{error}</code>
        </Card>
      )}
      {saved && (
        <Card glow="green" title="Saved">
          Settings updated.
        </Card>
      )}
      <Card title="Provider" glow="green">
        <Input
          label="Default Provider"
          value={settings.provider ?? ''}
          onChange={(e) => setSettings((s) => ({ ...s, provider: e.target.value }))}
          placeholder="e.g. openai"
          data-testid="settings-provider"
        />
        <Input
          label="Default Model"
          value={settings.model ?? ''}
          onChange={(e) => setSettings((s) => ({ ...s, model: e.target.value }))}
          placeholder="e.g. gpt-4o"
          data-testid="settings-model"
        />
        <Input
          label="API Key"
          type="password"
          value={settings.apiKey ?? ''}
          onChange={(e) => setSettings((s) => ({ ...s, apiKey: e.target.value }))}
          placeholder="sk-..."
        />
        <Button onClick={save} data-testid="settings-save">
          Save
        </Button>
      </Card>
    </div>
  );
}
