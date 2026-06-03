<script setup lang="ts">
import MobileLocalRead from './MobileLocalRead.vue'
import MobileComicStreamRead from './MobileComicStreamRead.vue'
import MobileComicStreamPanel from './MobileComicStreamPanel.vue'
import { comicReadSubTab, type ComicReadSubTab } from '../comicReadUi'
import { comicStreamReadSession, comicStreamSession, getFullComicStreamQueue } from '../comicStreamStore'
import {
  comicFavoritePlaylistsRevision,
  loadFavoriteComicStreamPlaylists,
} from '../comicStreamPlaylistStorage'
import { computed } from 'vue'

const comicSubTabs: { key: ComicReadSubTab; label: string }[] = [
  { key: 'home', label: '閱讀主頁' },
  { key: 'streamList', label: '串流列表' },
  { key: 'streamFav', label: '串流收藏' },
  { key: 'streamLog', label: '串流日誌' },
]

const fullQueueCount = computed(() => getFullComicStreamQueue().length)
const hasStreamLog = computed(() => comicStreamSession.logLines.length > 0)
const streamReadingActive = computed(() => comicStreamReadSession.readingActive)
const favCount = computed(() => {
  void comicFavoritePlaylistsRevision.value
  return loadFavoriteComicStreamPlaylists().length
})

function setComicSubTab(tab: ComicReadSubTab) {
  comicReadSubTab.value = tab
}
</script>

<template>
  <div class="comic-read-root">
    <nav class="comic-sub-nav" aria-label="漫畫閱讀子分頁">
      <button
        v-for="tab in comicSubTabs"
        :key="tab.key"
        type="button"
        class="comic-sub-link"
        :class="{ on: comicReadSubTab === tab.key }"
        @click="setComicSubTab(tab.key)"
      >
        <span class="comic-sub-link-text">{{ tab.label }}</span>
        <span v-if="tab.key === 'streamList' && fullQueueCount > 0" class="comic-sub-badge">{{ fullQueueCount }}</span>
        <span v-if="tab.key === 'streamFav' && favCount > 0" class="comic-sub-badge">{{ favCount }}</span>
        <span v-if="tab.key === 'streamLog' && hasStreamLog" class="comic-sub-dot" aria-hidden="true" />
      </button>
    </nav>

    <div class="comic-sub-body">
      <div v-show="comicReadSubTab === 'home'" class="comic-sub-pane comic-sub-pane--home">
        <MobileComicStreamRead key="stream-read" class="comic-stream-read" />
        <MobileLocalRead v-show="!streamReadingActive" key="read-home" class="comic-local-read" />
      </div>

      <div
        v-show="comicReadSubTab === 'streamList' || comicReadSubTab === 'streamFav' || comicReadSubTab === 'streamLog'"
        class="comic-sub-pane comic-sub-pane--stream-side"
      >
        <MobileComicStreamPanel
          v-if="comicReadSubTab === 'streamList'"
          key="stream-list"
          tab="streamList"
        />
        <MobileComicStreamPanel
          v-else-if="comicReadSubTab === 'streamFav'"
          key="stream-fav"
          tab="streamFav"
        />
        <MobileComicStreamPanel
          v-else-if="comicReadSubTab === 'streamLog'"
          key="stream-log"
          tab="streamLog"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.comic-read-root {
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  padding: 0;
}

.comic-sub-nav {
  display: flex;
  flex-shrink: 0;
  align-items: stretch;
  gap: 0;
  width: 100%;
  padding: 2px 0 0;
  box-sizing: border-box;
  border-bottom: 1px solid rgba(255, 255, 255, 0.1);
  background: rgba(0, 0, 0, 0.2);
}

.comic-sub-link {
  flex: 1 1 0;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: var(--gm-sub-tab-gap);
  padding: var(--gm-sub-tab-pad-y) var(--gm-sub-tab-pad-x);
  border: none;
  border-bottom: 2px solid transparent;
  background: transparent;
  color: #888;
  font-size: var(--gm-sub-tab-fs);
  line-height: 1.15;
  position: relative;
}

.comic-sub-link.on {
  color: #6eb5ff;
  border-bottom-color: #3d6ef5;
}

.comic-sub-link-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  text-align: center;
}

.comic-sub-badge {
  font-size: 9px;
  padding: 0 4px;
  border-radius: 8px;
  background: rgba(61, 110, 245, 0.35);
  color: #cfe0ff;
}

.comic-sub-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #6eb5ff;
}

.comic-sub-body {
  position: relative;
  flex: 1 1 auto;
  min-height: 0;
  width: 100%;
  overflow: hidden;
}

.comic-sub-pane {
  position: absolute;
  inset: 0;
  box-sizing: border-box;
  width: 100%;
  overflow-x: hidden;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
}

.comic-sub-pane--home {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.comic-sub-pane--stream-side {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.comic-local-read,
.comic-stream-read {
  flex: 1;
  min-height: 0;
  width: 100%;
  height: 100%;
}
</style>
