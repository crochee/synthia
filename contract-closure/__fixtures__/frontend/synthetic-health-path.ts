// intentionally calls a path that does NOT exist on the backend
export async function getSynthiaHealth(baseURL: string) {
  const res = await fetch(`${baseURL}/synthia/health`, { method: "GET" });
  return res.json();
}
