// test fixture: synthia-web frontend fetch usage
const HEALTH_URL = "/api/health";

export async function getHealth(baseURL: string) {
  const res = await fetch(`${baseURL}${HEALTH_URL}`, { method: "GET" });
  return res.json();
}

export async function listTasks(baseURL: string) {
  const res = await fetch(`${baseURL}/api/tasks`, { method: "GET" });
  return res.json();
}

export async function createTask(baseURL: string, body: unknown) {
  const res = await fetch(`${baseURL}/api/tasks`, {
    method: "POST",
    body: JSON.stringify(body),
  });
  return res.json();
}

export async function listTools(baseURL: string) {
  const res = await fetch(`${baseURL}/api/tools`, { method: "GET" });
  return res.json();
}

export async function a2aSendMessage(baseURL: string, request: unknown) {
  const res = await fetch(`${baseURL}/a2a/message:send`, {
    method: "POST",
    body: JSON.stringify(request),
  });
  return res.json();
}
