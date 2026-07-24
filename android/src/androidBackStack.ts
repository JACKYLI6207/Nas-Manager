/** Android 系統／邊緣返回：由各畫面註冊處理函式，回傳 true 表示已消化、勿關 App。 */

export type AndroidBackHandler = () => boolean

const handlers = new Set<AndroidBackHandler>()

export function registerAndroidBackHandler(handler: AndroidBackHandler): () => void {
  handlers.add(handler)
  return () => {
    handlers.delete(handler)
  }
}

export function tryHandleAndroidBack(): boolean {
  const list = [...handlers].reverse()
  for (const h of list) {
    try {
      if (h()) return true
    } catch {
      /* ignore */
    }
  }
  return false
}
