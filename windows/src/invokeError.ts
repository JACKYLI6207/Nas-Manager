/** 將 Tauri invoke 拒絕／CommandError 轉成可顯示字串 */
export function formatInvokeError(err: unknown): string {
  if (err == null) return '未知錯誤'
  if (typeof err === 'string') {
    const trimmed = err.trim()
    if (trimmed.startsWith('{')) {
      try {
        const parsed = formatInvokeErrorFromRecord(JSON.parse(trimmed) as Record<string, unknown>)
        if (parsed) return parsed
      } catch {
        /* use raw */
      }
    }
    return err
  }
  if (err instanceof Error) {
    const fromMsg = formatInvokeError(err.message)
    if (fromMsg && fromMsg !== '[object Object]') return fromMsg
    return err.message || '未知錯誤'
  }
  if (typeof err === 'object') {
    const parsed = formatInvokeErrorFromRecord(err as Record<string, unknown>)
    if (parsed) return parsed
    try {
      return JSON.stringify(err)
    } catch {
      return String(err)
    }
  }
  return String(err)
}

function formatInvokeErrorFromRecord(o: Record<string, unknown>): string | null {
  const errMessage = pickString(o, 'err_message', 'errMessage')
  const errTitle = pickString(o, 'err_title', 'errTitle')
  if (errMessage) {
    return errTitle ? `${errTitle}：${errMessage}` : errMessage
  }
  const message = o.message
  if (typeof message === 'string' && message.trim()) {
    return formatInvokeError(message)
  }
  if (message && typeof message === 'object') {
    const nested = formatInvokeErrorFromRecord(message as Record<string, unknown>)
    if (nested) return nested
  }
  const data = o.data
  if (data && typeof data === 'object') {
    const nested = formatInvokeErrorFromRecord(data as Record<string, unknown>)
    if (nested) return nested
  }
  return null
}

function pickString(o: Record<string, unknown>, ...keys: string[]): string | null {
  for (const key of keys) {
    const v = o[key]
    if (typeof v === 'string' && v.trim()) return v.trim()
  }
  return null
}
