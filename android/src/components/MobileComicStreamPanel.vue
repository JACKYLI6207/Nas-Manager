<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { copyTextFromTextarea } from '../copyText'
import {
  activateComicStreamQueueJob,
  clearComicStreamLog,
  clearCurrentComicStreamQueue,
  comicJobKey,
  comicStreamLog,
  comicStreamSession,
  getFullComicStreamQueue,
  removeFromComicStreamQueue,
} from '../comicStreamStore'
import {
  addFavoriteComicStreamPlaylist,
  loadFavoriteComicStreamPlaylists,
  removeFavoriteComicStreamPlaylist,
  removeFavoriteComicStreamPlaylistItem,
  renameFavoriteComicStreamPlaylist,
  type FavoriteComicStreamPlaylist,
} from '../comicStreamPlaylistStorage'
import {
  formatComicStreamProgressLabel,
  hasComicStreamRecord,
} from '../comicStreamProgress'
import { comicReadSubTab, type ComicReadSubTab } from '../comicReadUi'
import StreamPlaylistList from './StreamPlaylistList.vue'
import '../readerShared.css'

const props = defineProps<{
  tab: Exclude<ComicReadSubTab, 'home'>
}>()

const streamLogRef = ref<HTMLTextAreaElement | null>(null)
const copyLogHint = ref('')
const favoritePlaylists = ref<FavoriteComicStreamPlaylist[]>([])
const expandedFavoriteIds = ref<Set<string>>(new Set())

const streamLogText = computed(() => comicStreamSession.logLines.join('\n'))
const hasStreamLog = computed(() => comicStreamSession.logLines.length > 0)
const fullQueue = computed(() => getFullComicStreamQueue())
const fullQueueCount = computed(() => fullQueue.value.length)
const currentJobKey = computed(() =>
  comicStreamSession.currentJob ? comicJobKey(comicStreamSession.currentJob) : null,
)

function reloadFavoritePlaylists() {
  favoritePlaylists.value = loadFavoriteComicStreamPlaylists()
}

function isFavoriteExpanded(id: string): boolean {
  return expandedFavoriteIds.value.has(id)
}

function toggleFavoriteExpanded(id: string) {
  const next = new Set(expandedFavoriteIds.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expandedFavoriteIds.value = next
}

function streamProgressLabel(job: RemoteComicStreamJob): string | null {
  return formatComicStreamProgressLabel(job)
}

function streamOpenedMark(job: RemoteComicStreamJob): boolean {
  return hasComicStreamRecord(job)
}

function onPickQueueJob(job: (typeof fullQueue.value)[number]) {
  activateComicStreamQueueJob(job)
  comicReadSubTab.value = 'home'
}

function onRemoveQueueJob(job: (typeof fullQueue.value)[number]) {
  removeFromComicStreamQueue(job)
}

function onClearQueue() {
  if (!window.confirm('確定清空串流列表？')) return
  clearCurrentComicStreamQueue()
}

function onFavoriteCurrentList() {
  const jobs = getFullComicStreamQueue()
  if (jobs.length === 0) return
  const suggested = `漫畫 ${new Date().toLocaleDateString('zh-TW')}`
  const name = window.prompt('收藏列表名稱', suggested)
  if (name === null) return
  favoritePlaylists.value = addFavoriteComicStreamPlaylist(name, jobs)
  comicStreamLog(`已加入串流收藏：${name.trim() || suggested}`)
}

function onLoadFavorite(pl: FavoriteComicStreamPlaylist) {
  comicStreamSession.jobs = pl.jobs.map((j) => ({ ...j }))
  comicStreamSession.currentJob = null
  comicStreamSession.active = true
  comicStreamLog(`載入串流收藏：${pl.name}（${pl.jobs.length} 本）`)
  comicReadSubTab.value = 'streamList'
}

function onPickFavoriteJob(pl: FavoriteComicStreamPlaylist, job: (typeof pl.jobs)[number]) {
  activateComicStreamQueueJob(job, pl.jobs)
  comicReadSubTab.value = 'home'
}

function onRenameFavorite(id: string, currentName: string) {
  const name = window.prompt('重新命名', currentName)
  if (name === null) return
  favoritePlaylists.value = renameFavoriteComicStreamPlaylist(id, name)
}

function onRemoveFavorite(id: string) {
  if (!window.confirm('確定刪除此收藏？')) return
  favoritePlaylists.value = removeFavoriteComicStreamPlaylist(id)
}

function onRemoveFavoriteItem(plId: string, job: FavoriteComicStreamPlaylist['jobs'][number]) {
  favoritePlaylists.value = removeFavoriteComicStreamPlaylistItem(plId, job)
}

async function copyStreamLog() {
  const text = streamLogText.value
  if (!text.trim()) return
  const ok = await copyTextFromTextarea(text, streamLogRef.value)
  copyLogHint.value = ok ? '已複製' : '複製失敗'
  window.setTimeout(() => {
    copyLogHint.value = ''
  }, 2500)
}

onMounted(() => {
  if (props.tab === 'streamFav') reloadFavoritePlaylists()
})
</script>

<template>
  <div class="comic-stream-panel">
    <!-- 串流列表 -->
    <div v-show="tab === 'streamList'" class="comic-stream-pane">
      <div class="comic-stream-toolbar">
        <p class="comic-stream-heading">串流列表（{{ fullQueueCount }}）</p>
        <div class="comic-stream-toolbar-actions">
          <button
            type="button"
            class="reader-btn reader-btn--fit"
            :disabled="fullQueueCount === 0"
            @click="onFavoriteCurrentList"
          >
            ★ 收藏此列表
          </button>
          <button
            type="button"
            class="reader-btn reader-btn--fit"
            :disabled="fullQueueCount === 0"
            @click="onClearQueue"
          >
            清空列表
          </button>
        </div>
      </div>
      <StreamPlaylistList
        fill
        :jobs="fullQueue"
        :current-key="currentJobKey"
        :progress-for-job="streamProgressLabel"
        :opened-for-job="streamOpenedMark"
        show-remove
        empty-text="尚無項目；在遠端管理勾選 ZIP/CBZ 後「串流 → 串流閱讀」"
        @play="onPickQueueJob"
        @remove="onRemoveQueueJob"
      />
    </div>

    <!-- 串流收藏 -->
    <div v-show="tab === 'streamFav'" class="comic-stream-pane">
      <p class="comic-stream-heading comic-stream-heading--solo">串流收藏（{{ favoritePlaylists.length }}）</p>
      <div class="fav-pl-scroll">
        <p v-if="favoritePlaylists.length === 0" class="stream-pl-empty">
          尚無收藏；在「串流列表」按「★ 收藏此列表」
        </p>
        <div v-for="pl in favoritePlaylists" :key="pl.id" class="fav-pl-block">
          <div class="fav-pl-head">
            <button type="button" class="fav-pl-toggle" @click="toggleFavoriteExpanded(pl.id)">
              {{ isFavoriteExpanded(pl.id) ? '▼' : '▶' }}
            </button>
            <button type="button" class="fav-pl-name" @click="onLoadFavorite(pl)">{{ pl.name }}</button>
            <span class="fav-pl-count">{{ pl.jobs.length }} 本</span>
            <button type="button" class="fav-pl-mini" @click="onRenameFavorite(pl.id, pl.name)">改名</button>
            <button type="button" class="fav-pl-mini fav-pl-mini--danger" @click="onRemoveFavorite(pl.id)">刪</button>
          </div>
          <StreamPlaylistList
            v-if="isFavoriteExpanded(pl.id)"
            fill
            :jobs="pl.jobs"
            :progress-for-job="streamProgressLabel"
            :opened-for-job="streamOpenedMark"
            show-remove
            @play="(job) => onPickFavoriteJob(pl, job)"
            @remove="(job) => onRemoveFavoriteItem(pl.id, job)"
          />
        </div>
      </div>
    </div>

    <!-- 串流日誌 -->
    <div v-show="tab === 'streamLog'" class="comic-stream-pane comic-stream-pane--log">
      <div class="comic-stream-log-toolbar">
        <p class="comic-stream-heading">串流日誌</p>
        <div class="comic-stream-toolbar-actions">
          <button type="button" class="reader-btn reader-btn--fit" :disabled="!hasStreamLog" @click="copyStreamLog">
            複製
          </button>
          <button type="button" class="reader-btn reader-btn--fit" :disabled="!hasStreamLog" @click="clearComicStreamLog">
            清空
          </button>
        </div>
      </div>
      <p v-if="copyLogHint" class="copy-log-hint">{{ copyLogHint }}</p>
      <textarea
        ref="streamLogRef"
        class="stream-log-text"
        readonly
        :value="streamLogText"
        placeholder="（尚無日誌）"
      />
    </div>
  </div>
</template>

<style scoped>
.comic-stream-panel {
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  padding: 8px 10px 12px;
}
.comic-stream-pane {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  width: 100%;
}
.comic-stream-pane--log {
  overflow: hidden;
}
.comic-stream-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-shrink: 0;
  margin-bottom: 8px;
}
.comic-stream-log-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-shrink: 0;
  margin-bottom: 6px;
}
.comic-stream-toolbar-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  justify-content: flex-end;
}
.comic-stream-heading {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}
.comic-stream-heading--solo {
  flex-shrink: 0;
  margin-bottom: 8px;
}
.fav-pl-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.fav-pl-block {
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 10px;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-height: 0;
}
.fav-pl-head {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.fav-pl-toggle {
  border: none;
  background: transparent;
  color: inherit;
  padding: 0 4px;
}
.fav-pl-name {
  flex: 1;
  min-width: 0;
  text-align: left;
  border: none;
  background: transparent;
  color: #6eb5ff;
  font-weight: 600;
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.fav-pl-count {
  font-size: 11px;
  opacity: 0.65;
}
.fav-pl-mini {
  font-size: 11px;
  padding: 2px 6px;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  background: rgba(0, 0, 0, 0.2);
  color: inherit;
}
.fav-pl-mini--danger {
  color: #ffb4b4;
}
.stream-pl-empty {
  margin: 8px 0;
  font-size: 12px;
  opacity: 0.7;
}
.stream-log-text {
  flex: 1;
  min-height: 120px;
  width: 100%;
  box-sizing: border-box;
  resize: none;
  font-family: ui-monospace, monospace;
  font-size: 11px;
  line-height: 1.45;
  padding: 8px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(0, 0, 0, 0.25);
  color: inherit;
}
.copy-log-hint {
  margin: 0 0 4px;
  font-size: 11px;
  color: #8bc;
}
</style>
