<script setup lang="ts">
import { computed, ref, watch } from 'vue'
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
  type RemoteComicStreamJob,
} from '../comicStreamStore'
import {
  comicFavoriteAdapter,
  comicFavoritePlaylistsRevision,
} from '../comicStreamPlaylistStorage'
import {
  formatComicStreamProgressLabel,
  hasComicStreamRecord,
} from '../comicStreamProgress'
import { comicReadSubTab, type ComicReadSubTab } from '../comicReadUi'
import StreamPlaylistList from './StreamPlaylistList.vue'
import StreamFavoritesPanel from './StreamFavoritesPanel.vue'
import StreamFavoriteAddDialog from './StreamFavoriteAddDialog.vue'
import '../readerShared.css'

const props = defineProps<{
  tab: Exclude<ComicReadSubTab, 'home'>
}>()

const streamLogRef = ref<HTMLTextAreaElement | null>(null)
const copyLogHint = ref('')
const addDialogOpen = ref(false)
const addDialogJobs = ref<RemoteComicStreamJob[]>([])
/** 首次進入串流收藏才掛載面板，之後保留，避免列表↔收藏重掛卡頓 */
const favoritesEverOpened = ref(props.tab === 'streamFav')
watch(
  () => props.tab,
  (tab) => {
    if (tab === 'streamFav') favoritesEverOpened.value = true
  },
)

const streamLogText = computed(() => comicStreamSession.logLines.join('\n'))
const hasStreamLog = computed(() => comicStreamSession.logLines.length > 0)
const fullQueue = computed(() => getFullComicStreamQueue())
const fullQueueCount = computed(() => fullQueue.value.length)
const currentJobKey = computed(() =>
  comicStreamSession.currentJob ? comicJobKey(comicStreamSession.currentJob) : null,
)

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
  addDialogJobs.value = jobs
  addDialogOpen.value = true
}

function onAddDialogDone() {
  comicStreamLog('已更新串流收藏')
}

function onLoadFavoritePlaylist(jobs: RemoteComicStreamJob[], name: string) {
  if (jobs.length === 0) return
  comicStreamSession.jobs = jobs.map((j) => ({ ...j }))
  comicStreamSession.currentJob = null
  comicStreamSession.active = true
  comicStreamLog(`載入串流收藏：${name}（${jobs.length} 本）`)
  comicReadSubTab.value = 'streamList'
}

function onPlayFavoriteItem(job: RemoteComicStreamJob, sourceJobs: RemoteComicStreamJob[]) {
  activateComicStreamQueueJob(job, sourceJobs, { fromFavorites: true })
  comicReadSubTab.value = 'home'
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
        empty-text="尚無項目；在遠端管理勾選 ZIP/CBZ 後按「串流」"
        @play="onPickQueueJob"
        @remove="onRemoveQueueJob"
      />
    </div>

    <!-- 串流收藏（主分類 → 收藏名稱 → 項目） -->
    <div v-show="tab === 'streamFav'" class="comic-stream-pane">
      <StreamFavoritesPanel
        v-if="favoritesEverOpened"
        :adapter="comicFavoriteAdapter"
        media="comic"
        :current-key="currentJobKey"
        :revision="comicFavoritePlaylistsRevision"
        :progress-for-job="streamProgressLabel"
        :opened-for-job="streamOpenedMark"
        empty-hint="尚無收藏；在「串流列表」按「★ 收藏此列表」"
        @load-playlist="onLoadFavoritePlaylist"
        @play-item="onPlayFavoriteItem"
      />
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

    <StreamFavoriteAddDialog
      :open="addDialogOpen"
      :adapter="comicFavoriteAdapter"
      :jobs="addDialogJobs"
      default-name-fallback="漫畫收藏"
      @close="addDialogOpen = false"
      @done="onAddDialogDone"
    />
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
