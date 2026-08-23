// test fixture: synthia-web frontend fetch usage
const HEALTH_URL = "/api/health";

export async function getHealth(baseURL: string) {
  const res = await fetch(`${baseURL}${HEALTH_URL}`, { method: "GET" });
  return res.json();
}

export async function listSessions(baseURL: string) {
  const res = await fetch(`${baseURL}/api/v1/sessions`, { method: "GET" });
  return res.json();
}

export async function getSession(baseURL: string, id: string) {
  const res = await fetch(`${baseURL}/api/v1/sessions/${id}`, { method: "GET" });
  return res.json();
}

export async function sendChatMessage(
  baseURL: string,
  sessionId: string,
  body: unknown,
) {
  const res = await fetch(
    `${baseURL}/api/v1/chat/sessions/${sessionId}/messages`,
    {
      method: "POST",
      body: JSON.stringify(body),
    },
  );
  return res.json();
}

export async function listTools(baseURL: string) {
  const res = await fetch(`${baseURL}/api/tools`, { method: "GET" });
  return res.json();
}
