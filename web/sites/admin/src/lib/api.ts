export async function load<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`);
  if (!response.ok) throw new Error(`${path} answered ${response.status}`);
  return response.json();
}
