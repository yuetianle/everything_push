const BASE = '/api'

export async function api<T>(path: string, opts: RequestInit = {}): Promise<T> {
  const res = await fetch(BASE + path, {
    headers: { 'Content-Type': 'application/json', ...(opts.headers ?? {}) },
    ...opts,
  })
  if (res.status === 401) {
    location.href = '/login'
    throw new Error('未登录')
  }
  if (!res.ok) {
    const body = await res.json().catch(() => ({}) as { error?: string })
    throw new Error(body.error ?? res.statusText)
  }
  return res.json() as Promise<T>
}

export const apiGet = <T>(p: string) => api<T>(p)
export const apiPost = <T>(p: string, body: unknown) =>
  api<T>(p, { method: 'POST', body: JSON.stringify(body) })
export const apiPut = <T>(p: string, body: unknown) =>
  api<T>(p, { method: 'PUT', body: JSON.stringify(body) })
export const apiDel = <T>(p: string) => api<T>(p, { method: 'DELETE' })
