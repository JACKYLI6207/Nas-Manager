import { confirm, message } from '@tauri-apps/plugin-dialog'

const DEFAULT_TITLE = 'Nas Manager'

/** PC WebView2 的 window.confirm 會未等使用者就回傳 true；改用 Tauri 原生對話框 */
export async function pcConfirm(text: string, title = DEFAULT_TITLE): Promise<boolean> {
  try {
    return await confirm(text, { title, kind: 'warning' })
  } catch {
    return false
  }
}

export async function pcMessage(text: string, title = DEFAULT_TITLE): Promise<void> {
  try {
    await message(text, { title })
  } catch {
    /* ignore */
  }
}
