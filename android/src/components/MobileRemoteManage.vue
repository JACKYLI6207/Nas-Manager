<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  enterRemoteWifiMode,
  leaveRemoteWifiMode,
  scanLanRemotePcs,
  testRemotePcConnection,
  type RemotePcListItem,
} from '../api'
import { copyTextFromTextarea } from '../copyText'
import {
  addRemotePcFavorite,
  getRemotePcFavoriteDisplayName,
  isRemotePcFavorite,
  loadRemotePcFavorites,
  toggleRemotePcFavorite,
  updateRemotePcFavoriteName,
  type RemotePcFavorite,
} from '../remotePcFavoritesStorage'
import MobileRemoteBrowse from './MobileRemoteBrowse.vue'

const props = defineProps<{
  /** 主頁且 subNav=remote 時才 Teleport 底欄至 bottom-dock */
  homeDockActive: boolean
}>()

const scanning = ref(false)
const status = ref('')
const pcs = ref<RemotePcListItem[]>([])
const browsePc = ref<RemotePcListItem | null>(null)
const rootRef = ref<HTMLElement | null>(null)

function syncBrowseScrollClass() {
  const scroll = rootRef.value?.closest('.remote-manage-scroll')
  scroll?.classList.toggle('remote-manage-scroll--browse', !!browsePc.value)
}

watch(browsePc, syncBrowseScrollClass)
const scanLog = ref('')
/** 預設收合 LOG，避免佔滿畫面造成底部空白感 */
const showScanLog = ref(false)
const copyLogHint = ref('')
const scanLogRef = ref<HTMLTextAreaElement | null>(null)
const manualIp = ref('')
const manualPort = ref(8765)
const manualTesting = ref(false)
const favorites = ref<RemotePcFavorite[]>([])
const showFavoritesExpanded = ref(false)
const favoriteConnectingKey = ref('')
let scanGeneration = 0

const scanLogCopyText = computed(() => scanLog.value)

function favoriteKey(host: string, port: number) {
  return `${host.trim().toLowerCase()}:${port}`
}

function isFavoritePc(host: string, port: number) {
  return isRemotePcFavorite(host, port, favorites.value)
}

function reloadFavorites() {
  favorites.value = loadRemotePcFavorites()
}

function displayNameForPc(pc: RemotePcListItem) {
  const host = pc.connectedHost ?? pc.hosts[0] ?? ''
  if (!host) return pc.name
  return getRemotePcFavoriteDisplayName(host, pc.port, pc.name)
}

function applyFavoriteNameToPcList(host: string, port: number, name: string) {
  const display = name.trim()
  if (!display) return
  pcs.value = pcs.value.map((pc) => {
    const h = pc.connectedHost ?? pc.hosts[0] ?? ''
    if (pc.port === port && h === host) {
      return { ...pc, name: display }
    }
    return pc
  })
}

function saveFavoriteName(fav: RemotePcFavorite, rawName: string) {
  const trimmed = rawName.trim()
  if (!trimmed || trimmed === fav.name) return
  favorites.value = updateRemotePcFavoriteName(fav.host, fav.port, trimmed)
  applyFavoriteNameToPcList(fav.host, fav.port, trimmed)
}

function toggleFavorite(pc: { name: string; hosts: string[]; port: number; connectedHost?: string | null }) {
  const host = pc.connectedHost ?? pc.hosts[0] ?? ''
  if (!host) return
  if (isFavoritePc(host, pc.port)) {
    favorites.value = toggleRemotePcFavorite(pc.name, host, pc.port)
    return
  }
  const suggested = displayNameForPc({ ...pc, connectedHost: host, connected: true })
  const input = window.prompt('請為此 PC 取名（方便辨認）', suggested)
  if (input === null) return
  const customName = input.trim() || suggested
  favorites.value = addRemotePcFavorite(customName, host, pc.port)
  applyFavoriteNameToPcList(host, pc.port, customName)
}

async function connectToHost(
  ip: string,
  port: number,
  displayName?: string,
  options?: { appendLog?: boolean; preferListFront?: boolean },
) {
  const result = await testRemotePcConnection([ip], port, true)
  if (options?.appendLog !== false) {
    scanLog.value += `\n\n--- 連線 ---\n${ip}:${port} → ${result.message}`
    showScanLog.value = true
  }
  if (!result.connected) {
    return { ok: false as const, message: result.message }
  }
  const item: RemotePcListItem = {
    name: getRemotePcFavoriteDisplayName(
      result.connectedHost ?? ip,
      port,
      displayName?.trim() || `PC (${ip})`,
    ),
    hosts: [ip],
    port,
    connected: true,
    message: result.message,
    connectedHost: result.connectedHost ?? ip,
  }
  const existing = pcs.value.findIndex((p) => p.port === port && p.hosts.includes(ip))
  if (existing >= 0) {
    pcs.value[existing] = item
  } else if (options?.preferListFront) {
    pcs.value = [item, ...pcs.value]
  } else {
    pcs.value.push(item)
  }
  return { ok: true as const, item, message: result.message }
}

function appendLog(section: string, lines: string[]) {
  if (!scanLog.value) return
  scanLog.value += `\n\n--- ${section} ---\n${lines.join('\n')}`
}

async function copyScanLog() {
  const text = scanLogCopyText.value
  if (!text.trim()) return
  copyLogHint.value = ''
  const ok = await copyTextFromTextarea(text, scanLogRef.value)
  copyLogHint.value = ok ? '已複製到剪貼簿' : '複製失敗，請再試一次'
  window.setTimeout(() => {
    copyLogHint.value = ''
  }, 2000)
}

async function connectFavoritePcsFirst(): Promise<number> {
  const favs = favorites.value
  if (favs.length === 0) return 0
  let ok = 0
  status.value = `正在連線已收藏的 ${favs.length} 台 PC…`
  for (const fav of favs) {
    const key = favoriteKey(fav.host, fav.port)
    favoriteConnectingKey.value = key
    try {
      const result = await connectToHost(fav.host, fav.port, fav.name, {
        appendLog: false,
        preferListFront: true,
      })
      if (result.ok) ok++
    } catch {
      /* 單台失敗不阻斷 */
    }
  }
  favoriteConnectingKey.value = ''
  return ok
}

function syncBrowsePcFromList() {
  const cur = browsePc.value
  if (!cur) return
  const host = cur.connectedHost ?? cur.hosts[0] ?? ''
  if (!host) return
  const updated = pcs.value.find(
    (p) =>
      p.port === cur.port &&
      p.connected === true &&
      (p.connectedHost === host ||
        p.hosts.includes(host) ||
        (cur.connectedHost != null && p.hosts.includes(cur.connectedHost))),
  )
  if (updated) {
    browsePc.value = { ...updated, name: displayNameForPc(updated) }
  }
}

async function refreshList(options?: { resetBrowse?: boolean }) {
  const gen = ++scanGeneration
  const preservedConnected = pcs.value.filter((p) => p.connected === true)
  scanning.value = true
  status.value = '正在掃描區網內的 PC…'
  pcs.value = [...preservedConnected]
  if (options?.resetBrowse) {
    browsePc.value = null
  }
  scanLog.value = ''
  copyLogHint.value = ''
  try {
    const result = await scanLanRemotePcs()
    if (gen !== scanGeneration) return
    scanLog.value = result.log
    showScanLog.value = true

    const discovered = result.pcs
    if (discovered.length === 0) {
      status.value =
        '未找到 PC。請查看下方 LOG 並複製回報，或確認：① 同一 Wi‑Fi ② PC 遠端管理「執行中」③ 防火牆允許私人網路。'
      syncBrowsePcFromList()
      return
    }
    const merged: RemotePcListItem[] = [...preservedConnected]
    for (const pc of discovered) {
      const dup = merged.findIndex(
        (p) =>
          p.port === pc.port &&
          (p.hosts.some((h) => pc.hosts.includes(h)) ||
            (pc.connectedHost != null && p.hosts.includes(pc.connectedHost))),
      )
      if (dup >= 0) {
        merged[dup] = {
          ...pc,
          connected: null,
          message: '測試連線中…',
          connectedHost: null,
          name: getRemotePcFavoriteDisplayName(
            pc.connectedHost ?? pc.hosts[0] ?? '',
            pc.port,
            displayNameForPc(pc),
          ),
        }
      } else {
        merged.push({
          ...pc,
          connected: null,
          message: '測試連線中…',
          connectedHost: null,
        })
      }
    }
    pcs.value = merged
    status.value = `找到 ${discovered.length} 台 PC，正在測試連線…`
    const testLines: string[] = []
    await Promise.all(
      pcs.value.map(async (pc, index) => {
        const result = await testRemotePcConnection(pc.hosts, pc.port)
        if (gen !== scanGeneration) return
        const connectedHost = result.connectedHost
        pcs.value[index] = {
          ...pc,
          connected: result.connected,
          message: result.message,
          connectedHost,
          name:
            result.connected && connectedHost
              ? getRemotePcFavoriteDisplayName(connectedHost, pc.port, pc.name)
              : pc.name,
        }
        testLines.push(
          `${pc.name} (${pc.hosts.join(' / ')}:${pc.port}) → ${
            result.connected ? `OK ${result.connectedHost ?? ''}` : result.message
          }`,
        )
      }),
    )
    if (gen !== scanGeneration) return
    appendLog('連線測試', testLines)
    const okCount = pcs.value.filter((p) => p.connected === true).length
    status.value = `掃描完成：${okCount} / ${pcs.value.length} 台能連線`
    showScanLog.value = okCount === 0
    syncBrowsePcFromList()
  } catch (e) {
    if (gen !== scanGeneration) return
    status.value = String(e)
    scanLog.value += `\n\n--- 錯誤 ---\n${String(e)}`
    showScanLog.value = true
  } finally {
    if (gen === scanGeneration) scanning.value = false
  }
}

function openManage(pc: RemotePcListItem) {
  if (pc.connected !== true) return
  browsePc.value = { ...pc, name: displayNameForPc(pc) }
}

async function connectManualIp() {
  const ip = manualIp.value.trim()
  if (!ip) {
    status.value = '請輸入 PC 的 IP 位址（區網 192.168.x.x 或 Tailscale 100.x.x.x）'
    return
  }
  manualTesting.value = true
  status.value = `正在連線 ${ip}:${manualPort.value}…`
  try {
    const result = await connectToHost(ip, manualPort.value, `PC (${ip})`, {
      preferListFront: true,
    })
    if (!result.ok) {
      status.value = result.message
      return
    }
    status.value = `已連線 ${ip}，可點「管理」進入`
  } catch (e) {
    status.value = String(e)
  } finally {
    manualTesting.value = false
  }
}

async function connectFavorite(fav: RemotePcFavorite) {
  const key = favoriteKey(fav.host, fav.port)
  favoriteConnectingKey.value = key
  status.value = `正在連線 ${fav.name}（${fav.host}:${fav.port}）…`
  try {
    const result = await connectToHost(fav.host, fav.port, fav.name, { preferListFront: true })
    status.value = result.ok
      ? `已連線 ${fav.name}，可點「管理」進入`
      : result.message
  } catch (e) {
    status.value = String(e)
  } finally {
    favoriteConnectingKey.value = ''
  }
}

onBeforeUnmount(() => {
  scanGeneration++
  rootRef.value?.closest('.remote-manage-scroll')?.classList.remove('remote-manage-scroll--browse')
  void leaveRemoteWifiMode()
})

onMounted(async () => {
  reloadFavorites()
  try {
    const wifiMsg = await enterRemoteWifiMode()
    scanLog.value = `[Wi‑Fi 區網模式] ${wifiMsg}\n`
    showScanLog.value = true
  } catch {
    // ignore
  }
  const favOk = await connectFavoritePcsFirst()
  if (favOk > 0) {
    status.value = `已連線 ${favOk} 台收藏 PC，接著掃描區網…`
  }
  void refreshList()
  syncBrowseScrollClass()
})
</script>

<template>
  <div ref="rootRef" class="remote-manage-root" :class="{ 'remote-manage-root--browse': !!browsePc }">
  <div v-if="browsePc" class="remote-browse-shell">
    <MobileRemoteBrowse
      :pc="browsePc"
      :dock-foot-enabled="homeDockActive"
      @exit="browsePc = null"
    />
  </div>
  <div v-else class="remote-manage">
    <div class="remote-manage-toolbar">
      <p class="remote-manage-hint">
        <strong>同一 Wi‑Fi</strong>：按「重新掃描」或手動輸入區網 IP（192.168.x.x）。
        <strong>4G／跨網（Tailscale）</strong>：手機與 PC 皆開 Tailscale，手動輸入 PC 的
        <code>100.x.x.x</code> 後按「連線」（不必關 4G）。
        可將已連線的 PC 按 ★ 收藏並<strong>自訂名稱</strong>；下方列表與管理頁會顯示該名稱。
      </p>
      <button type="button" class="tool tool--primary" :disabled="scanning" @click="refreshList({ resetBrowse: true })">
        {{ scanning ? '掃描中…' : '重新掃描' }}
      </button>
      <div v-if="favorites.length > 0" class="remote-favorites">
        <button
          type="button"
          class="remote-favorites-toggle"
          @click="showFavoritesExpanded = !showFavoritesExpanded"
        >
          {{ showFavoritesExpanded ? '▼' : '▶' }} ★ 已收藏 PC（{{ favorites.length }}，可編輯名稱）
        </button>
        <ul v-show="showFavoritesExpanded" class="remote-favorites-list">
          <li
            v-for="fav in favorites"
            :key="`${fav.host}:${fav.port}`"
            class="remote-favorite-item"
          >
            <div class="remote-favorite-main">
              <input
                class="remote-favorite-name-input"
                type="text"
                :value="fav.name"
                maxlength="64"
                placeholder="自訂名稱"
                :disabled="favoriteConnectingKey !== '' || manualTesting || scanning"
                @change="saveFavoriteName(fav, ($event.target as HTMLInputElement).value)"
                @blur="saveFavoriteName(fav, ($event.target as HTMLInputElement).value)"
              />
              <span class="remote-favorite-addr">{{ fav.host }}:{{ fav.port }}</span>
            </div>
            <div class="remote-favorite-actions">
              <button
                type="button"
                class="remote-favorite-star on"
                title="取消收藏"
                :disabled="favoriteConnectingKey !== '' || manualTesting || scanning"
                @click="favorites = toggleRemotePcFavorite(fav.name, fav.host, fav.port)"
              >
                ★
              </button>
              <button
                type="button"
                class="tool remote-favorite-connect"
                :disabled="favoriteConnectingKey !== '' || manualTesting || scanning"
                @click="connectFavorite(fav)"
              >
                {{
                  favoriteConnectingKey === `${fav.host.toLowerCase()}:${fav.port}`
                    ? '連線中…'
                    : '連線'
                }}
              </button>
            </div>
          </li>
        </ul>
      </div>
      <div class="remote-manual-connect">
        <p class="remote-manual-label">手動輸入 PC IP（區網或 Tailscale 100.x.x.x）</p>
        <div class="remote-manual-row">
          <input
            v-model="manualIp"
            class="remote-manual-input"
            type="text"
            inputmode="decimal"
            placeholder="100.x.x.x 或 192.168.x.x"
            :disabled="manualTesting || scanning"
          />
          <input
            v-model.number="manualPort"
            class="remote-manual-port"
            type="number"
            min="1"
            max="65535"
            :disabled="manualTesting || scanning"
          />
          <button
            type="button"
            class="tool remote-manual-btn"
            :disabled="manualTesting || scanning"
            @click="connectManualIp"
          >
            {{ manualTesting ? '連線中…' : '連線' }}
          </button>
        </div>
      </div>
    </div>
    <p v-if="status" class="remote-manage-status">{{ status }}</p>
    <ul v-if="pcs.length > 0" class="remote-pc-list">
      <li v-for="(pc, i) in pcs" :key="`${pc.hosts.join(',')}:${pc.port}:${i}`" class="remote-pc-item">
        <div class="remote-pc-main">
          <span class="remote-pc-name">{{ displayNameForPc(pc) }}</span>
          <span class="remote-pc-addr">{{ pc.hosts.join(' / ') }}:{{ pc.port }}</span>
        </div>
        <div class="remote-pc-footer">
          <button
            v-if="pc.connected === true"
            type="button"
            class="remote-pc-star"
            :class="{ 'remote-pc-star--on': isFavoritePc(pc.connectedHost ?? pc.hosts[0] ?? '', pc.port) }"
            :title="isFavoritePc(pc.connectedHost ?? pc.hosts[0] ?? '', pc.port) ? '取消收藏' : '加入收藏'"
            @click="toggleFavorite(pc)"
          >
            {{ isFavoritePc(pc.connectedHost ?? pc.hosts[0] ?? '', pc.port) ? '★' : '☆' }}
          </button>
          <span
            class="remote-pc-badge"
            :class="{
              'remote-pc-badge--ok': pc.connected === true,
              'remote-pc-badge--fail': pc.connected === false,
              'remote-pc-badge--pending': pc.connected === null,
            }"
          >
            {{
              pc.connected === true
                ? '能連線'
                : pc.connected === false
                  ? '不能連線'
                  : '辨識中'
            }}
          </span>
          <button
            v-if="pc.connected === true"
            type="button"
            class="remote-pc-manage-btn"
            @click="openManage(pc)"
          >
            管理
          </button>
        </div>
        <p v-if="pc.message && pc.connected !== true" class="remote-pc-msg">{{ pc.message }}</p>
      </li>
    </ul>

    <div v-if="scanLog" class="remote-scan-log-wrap">
      <div class="remote-scan-log-header">
        <button type="button" class="remote-scan-log-toggle" @click="showScanLog = !showScanLog">
          {{ showScanLog ? '▼' : '▶' }} 掃描 LOG（可複製）
        </button>
        <div class="remote-scan-log-actions">
          <button type="button" class="tool tool--ghost remote-scan-copy" @click="copyScanLog">
            複製 LOG
          </button>
          <span v-if="copyLogHint" class="remote-scan-copy-hint">{{ copyLogHint }}</span>
        </div>
      </div>
      <textarea
        v-show="showScanLog"
        ref="scanLogRef"
        class="remote-scan-log"
        readonly
        :value="scanLog"
        rows="8"
      />
    </div>
  </div>
  </div>
</template>

<style scoped>
.remote-manage-root {
  display: block;
}

.remote-manage-root--browse {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
}

.remote-browse-shell {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 100%;
  width: 100%;
}

.remote-manage {
  padding: 12px 14px 16px;
}

.remote-manage-toolbar {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin-bottom: 12px;
}

.remote-manage-hint {
  margin: 0;
  font-size: 13px;
  line-height: 1.5;
  opacity: 0.85;
}

.remote-manual-connect {
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px dashed var(--gm-border, rgba(255, 255, 255, 0.18));
  background: rgba(0, 0, 0, 0.1);
}

.remote-favorites {
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px solid rgba(255, 215, 0, 0.25);
  background: rgba(255, 215, 0, 0.06);
}

.remote-favorites-toggle {
  width: 100%;
  margin: 0 0 8px;
  padding: 6px 4px;
  border: none;
  background: transparent;
  color: inherit;
  font-size: 12px;
  font-weight: 600;
  text-align: left;
  opacity: 0.9;
}

.remote-favorites-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.remote-favorite-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.15);
}

.remote-favorite-main {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.remote-favorite-name-input {
  width: 100%;
  box-sizing: border-box;
  padding: 4px 8px;
  border-radius: 6px;
  border: 1px solid rgba(255, 215, 0, 0.35);
  background: rgba(0, 0, 0, 0.2);
  color: inherit;
  font-weight: 600;
  font-size: 14px;
}

.remote-favorite-addr {
  font-size: 11px;
  opacity: 0.75;
  font-family: ui-monospace, monospace;
}

.remote-favorite-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.remote-favorite-star {
  border: none;
  background: transparent;
  color: #ffd700;
  font-size: 18px;
  line-height: 1;
  padding: 4px;
}

.remote-favorite-connect {
  padding: 6px 12px;
  font-size: 13px;
}

.remote-pc-star {
  flex-shrink: 0;
  border: none;
  background: transparent;
  color: rgba(255, 255, 255, 0.45);
  font-size: 18px;
  line-height: 1;
  padding: 0 4px 0 0;
}

.remote-pc-star--on {
  color: #ffd700;
}

.remote-manual-label {
  margin: 0 0 8px;
  font-size: 12px;
  opacity: 0.8;
}

.remote-manual-row {
  display: flex;
  gap: 8px;
  align-items: center;
}

.remote-manual-input {
  flex: 1;
  min-width: 0;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.15));
  background: rgba(0, 0, 0, 0.2);
  color: inherit;
  font-family: ui-monospace, monospace;
  font-size: 14px;
}

.remote-manual-port {
  width: 72px;
  padding: 8px 6px;
  border-radius: 8px;
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.15));
  background: rgba(0, 0, 0, 0.2);
  color: inherit;
  font-size: 14px;
}

.remote-manual-btn {
  flex-shrink: 0;
  padding: 8px 14px;
}

.remote-manage-status {
  margin: 0 0 12px;
  font-size: 13px;
  opacity: 0.8;
}

.remote-pc-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.remote-pc-item {
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.12));
  border-radius: 10px;
  padding: 12px;
  background: rgba(0, 0, 0, 0.15);
}

.remote-pc-main {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 8px;
}

.remote-pc-name {
  font-weight: 600;
  font-size: 15px;
}

.remote-pc-addr {
  font-size: 12px;
  opacity: 0.7;
  font-family: ui-monospace, monospace;
}

.remote-pc-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.remote-pc-badge {
  display: inline-block;
  font-size: 12px;
  padding: 2px 8px;
  border-radius: 6px;
  background: rgba(255, 255, 255, 0.1);
}

.remote-pc-badge--ok {
  background: rgba(72, 187, 120, 0.25);
  color: #9ae6b4;
}

.remote-pc-badge--fail {
  background: rgba(245, 101, 101, 0.2);
  color: #feb2b2;
}

.remote-pc-badge--pending {
  opacity: 0.75;
}

.remote-pc-manage-btn {
  flex-shrink: 0;
  padding: 6px 16px;
  border-radius: 8px;
  border: none;
  background: #3182ce;
  color: #fff;
  font-size: 14px;
  font-weight: 500;
}

.remote-pc-msg {
  margin: 8px 0 0;
  font-size: 12px;
  opacity: 0.7;
  line-height: 1.4;
}

.remote-scan-log-wrap {
  margin-top: 16px;
  border: 1px solid var(--gm-border, rgba(255, 255, 255, 0.12));
  border-radius: 10px;
  overflow: hidden;
  background: rgba(0, 0, 0, 0.2);
}

.remote-scan-log-header {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--gm-border, rgba(255, 255, 255, 0.08));
}

.remote-scan-log-toggle {
  border: none;
  background: transparent;
  color: inherit;
  font-size: 13px;
  font-weight: 600;
  padding: 4px 0;
}

.remote-scan-log-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.remote-scan-copy {
  font-size: 12px;
  padding: 4px 10px;
}

.remote-scan-copy-hint {
  font-size: 12px;
  opacity: 0.75;
  color: #9ae6b4;
}

.remote-scan-log {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin: 0;
  padding: 10px 12px;
  border: none;
  background: rgba(0, 0, 0, 0.25);
  color: inherit;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  line-height: 1.45;
  resize: vertical;
  min-height: 100px;
  max-height: 32vh;
  user-select: text;
  -webkit-user-select: text;
}
</style>
