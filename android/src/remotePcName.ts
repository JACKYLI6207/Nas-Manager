/**
 * 將「Steam Deck (區網)」「Steam Deck (偽區網)」等顯示名正規化為同一台 PC 的鍵。
 * 區網 / Tailscale（偽區網）只是連線路徑不同，不應拆成兩台機器。
 */
export function normalizePcName(name: string): string {
  return name
    .trim()
    .replace(/[（(]\s*偽?\s*區網\s*[）)]/g, '')
    .replace(/[（(]\s*Tailscale\s*[）)]/gi, '')
    .replace(/[（(]\s*TS\s*[）)]/gi, '')
    .replace(/\s+/g, ' ')
    .trim()
    .toLowerCase()
}

/** 判斷是否像 Tailscale CGNAT / 常見 100.x 位址（連線時可跳過 Wi‑Fi 綁定）。 */
export function looksLikeTailscaleHost(host: string): boolean {
  const h = host.trim()
  if (!h) return false
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(h)
  if (!m) return false
  const a = Number(m[1])
  const b = Number(m[2])
  // Tailscale CGNAT 100.64.0.0/10
  return a === 100 && b >= 64 && b <= 127
}
