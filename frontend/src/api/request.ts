declare global {
  interface Window {
    codexProxyPlugin: { theme: 'light' | 'dark', request: (input: { method: string, path: string, contentType?: string, body?: string }) => Promise<{ status: number, body: ArrayBuffer }> }
  }
}
export async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const result = await window.codexProxyPlugin.request({ method, path, ...(body === undefined ? {} : { contentType: 'application/json', body: JSON.stringify(body) }) })
  const value = JSON.parse(new TextDecoder().decode(result.body))
  if (result.status >= 400)
    throw new Error(value.error || '操作失败，请稍后重试')
  return value
}
