import { invoke } from '@tauri-apps/api/core'

function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** Android WebView 需走原生 ClipboardManager；桌面再 fallback DOM */
export async function copyTextFromTextarea(
  text: string,
  textarea?: HTMLTextAreaElement | null,
): Promise<boolean> {
  if (!text.trim()) return false

  if (isTauri()) {
    try {
      await invoke('copy_text_to_clipboard', { text })
      return true
    } catch {
      /* fallback */
    }
  }

  if (textarea) {
    textarea.focus()
    textarea.select()
    try {
      textarea.setSelectionRange(0, textarea.value.length)
    } catch {
      /* ignore */
    }
    try {
      if (document.execCommand('copy')) return true
    } catch {
      /* ignore */
    }
  }

  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    /* ignore */
  }

  return false
}
