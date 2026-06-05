<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { open } from '@tauri-apps/plugin-dialog'
import {
  commands,
  type Config,
  type RemoteManagementStatus,
  type ShareRootBinding,
} from '../bindings.ts'

function clampSlots(n: number): number {
  if (!Number.isFinite(n)) return 3
  return Math.min(16, Math.max(1, Math.round(n)))
}

const EMPTY_BINDING: ShareRootBinding = {
  volumeGuid: '',
  relativePath: '',
  displayHint: '',
}

function isBindingEmpty(b: ShareRootBinding): boolean {
  return !b.volumeGuid?.trim()
}

function ensureShareRoots(config: Config): ShareRootBinding[] {
  const slots = clampSlots(config.remoteManagementShareSlots ?? 3)
  const roots = [...(config.remoteManagementShareRoots ?? [])]
  while (roots.length < slots) roots.push({ ...EMPTY_BINDING })
  return roots.slice(0, slots)
}

const config = ref<Config | null>(null)
const status = ref<RemoteManagementStatus | null>(null)
const shareRoots = ref<ShareRootBinding[]>([])
const loading = ref(true)
const saving = ref(false)
const message = ref('')
let timer: ReturnType<typeof setInterval> | null = null
let autoSaveTimer: ReturnType<typeof setTimeout> | null = null

function syncShareRootsFromConfig() {
  if (!config.value) return
  shareRoots.value = ensureShareRoots(config.value)
}

function shareRootLabel(index: number): string {
  const binding = shareRoots.value[index]
  if (!binding || isBindingEmpty(binding)) return '（未指定）'
  const resolved = status.value?.shareDirs?.[index]
  if (resolved) return resolved
  if (binding.displayHint) return binding.displayHint
  if (binding.relativePath) {
    return `Volume {${binding.volumeGuid}} / ${binding.relativePath}`
  }
  return `Volume {${binding.volumeGuid}}`
}

function applyShareRootsToConfig() {
  if (!config.value) return
  const cleaned = shareRoots.value.filter((b) => !isBindingEmpty(b))
  config.value.remoteManagementShareRoots = cleaned
}

async function refreshStatus() {
  try {
    status.value = await commands.getRemoteManagementStatus()
  } catch {
    status.value = null
  }
}

async function load() {
  loading.value = true
  message.value = ''
  try {
    config.value = await commands.getConfig()
    if (config.value.remoteManagementShareSlots == null) {
      config.value.remoteManagementShareSlots = 3
    }
    if (!config.value.remoteManagementShareRoots) {
      config.value.remoteManagementShareRoots = []
    }
    syncShareRootsFromConfig()
    await refreshStatus()
  } catch (err) {
    config.value = null
    message.value =
      err instanceof Error ? `載入設定失敗：${err.message}` : '載入設定失敗，請重新啟動程式'
  } finally {
    loading.value = false
  }
}

async function save(silent = false) {
  if (!config.value) return
  applyShareRootsToConfig()
  config.value.remoteManagementShareSlots = clampSlots(
    config.value.remoteManagementShareSlots ?? 3,
  )
  saving.value = true
  if (!silent) message.value = ''
  try {
    const result = await commands.saveConfig(config.value)
    if (result.status === 'ok') {
      message.value = silent ? '已自動儲存' : '設定已儲存'
      await refreshStatus()
    } else {
      message.value = result.error.err_message || '儲存失敗'
    }
  } finally {
    saving.value = false
  }
}

function scheduleAutoSave() {
  if (autoSaveTimer != null) clearTimeout(autoSaveTimer)
  autoSaveTimer = window.setTimeout(() => {
    autoSaveTimer = null
    void save(true)
  }, 400)
}

watch(
  shareRoots,
  () => {
    if (loading.value) return
    scheduleAutoSave()
  },
  { deep: true },
)

watch(
  () => config.value?.remoteManagementShareSlots,
  () => {
    if (!config.value || loading.value) return
    config.value.remoteManagementShareSlots = clampSlots(
      config.value.remoteManagementShareSlots ?? 3,
    )
    syncShareRootsFromConfig()
    scheduleAutoSave()
  },
)

async function restartRemote() {
  message.value = ''
  const result = await commands.restartRemoteManagement()
  if (result.status === 'ok') {
    status.value = result.data
    message.value = '已重新啟動遠端服務'
  } else {
    message.value = result.error.err_message || '重新啟動失敗'
  }
}

async function pickShareDir(index: number) {
  if (!config.value) return
  const picked = await open({ directory: true, multiple: false })
  if (typeof picked !== 'string' || picked.length === 0) return

  const result = await commands.bindShareRootPath(picked)
  if (result.status !== 'ok') {
    message.value = result.error.err_message || '綁定分享路徑失敗'
    return
  }

  const next = [...shareRoots.value]
  while (next.length <= index) next.push({ ...EMPTY_BINDING })
  next[index] = result.data.binding
  shareRoots.value = next
}

function clearShareDir(index: number) {
  const next = [...shareRoots.value]
  next[index] = { ...EMPTY_BINDING }
  shareRoots.value = next
}

onMounted(async () => {
  await load()
  timer = setInterval(() => {
    void refreshStatus()
  }, 2000)
})

onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
  if (autoSaveTimer != null) clearTimeout(autoSaveTimer)
})

const statusText = computed(() => (status.value?.running ? '執行中' : '未執行'))
</script>

<template>
  <div class="server-settings">
    <p class="server-settings-desc">
      本機作為區網伺服端：分享資料夾供 Android／其他 PC 瀏覽、串流與傳檔。
    </p>

    <p v-if="loading || !config">載入中…</p>
    <template v-else>
      <label class="server-row">
        <input
          v-model="config.remoteManagementEnabled"
          type="checkbox"
          @change="save(true)"
        />
        <span>開啟遠端管理（區網）</span>
      </label>

      <div class="server-row server-row--wrap">
        <span>分享資料夾數量：</span>
        <input
          v-model.number="config.remoteManagementShareSlots"
          type="number"
          min="1"
          max="16"
          class="server-input-num"
        />
        <span class="server-hint">（多個時，客戶端根目錄會列出各分享名稱）</span>
      </div>

      <div
        v-for="(_, index) in shareRoots"
        :key="index"
        class="server-share-row"
      >
        <input
          class="server-input-path"
          :value="shareRootLabel(index)"
          readonly
        />
        <button type="button" @click="pickShareDir(index)">指定 #{{ index + 1 }}</button>
        <button
          type="button"
          :disabled="isBindingEmpty(shareRoots[index] ?? EMPTY_BINDING)"
          @click="clearShareDir(index)"
        >
          清除
        </button>
      </div>

      <p class="server-hint">
        分享路徑以 Volume GUID 綁定，磁碟代號變更後仍可還原。更換路徑後會自動儲存；若服務已在執行，請按「重新啟動服務」。
      </p>

      <div class="server-row server-row--wrap">
        <span>連接埠：</span>
        <input
          v-model.number="config.remoteManagementPort"
          type="number"
          class="server-input-num"
        />
        <button type="button" :disabled="saving" @click="save(false)">
          {{ saving ? '儲存中…' : '儲存設定' }}
        </button>
        <button type="button" @click="restartRemote">重新啟動服務</button>
      </div>
    </template>

    <div class="server-status">
      <div>服務狀態：{{ statusText }}</div>
      <div>顯示名稱：{{ status?.displayName ?? '-' }}</div>
      <div>分享目錄：{{ status?.shareDir ?? '-' }}</div>
      <div>區網 IP：{{ status?.lanAddresses?.join('、') || '-' }}</div>
      <div>防火牆：{{ status?.firewallReady ? '已就緒' : '未確認' }}</div>
      <div v-if="status?.lastError" class="server-error">錯誤：{{ status.lastError }}</div>
      <div v-if="message" class="server-msg">{{ message }}</div>
    </div>
  </div>
</template>

<style scoped>
.server-settings {
  padding: 12px 14px 24px;
  color: #eee;
  font-size: 14px;
  line-height: 1.5;
}

.server-settings-desc {
  margin: 0 0 12px;
  color: #aaa;
}

.server-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 10px;
}

.server-row--wrap {
  flex-wrap: wrap;
}

.server-share-row {
  display: grid;
  grid-template-columns: 1fr auto auto;
  gap: 8px;
  margin-bottom: 8px;
  align-items: center;
}

.server-input-path,
.server-input-num {
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid #444;
  background: #121212;
  color: #e8e8e8;
}

.server-input-num {
  width: 72px;
}

.server-hint {
  margin: 0 0 10px;
  font-size: 12px;
  color: #888;
}

button {
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid #444;
  background: #2a2a2a;
  color: #eee;
  cursor: pointer;
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.server-status {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px solid #333;
  font-size: 13px;
}

.server-error {
  color: #ff7875;
}

.server-msg {
  margin-top: 8px;
  color: #95de64;
}
</style>
