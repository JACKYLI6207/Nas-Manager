<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { onBackButtonPress } from '@tauri-apps/api/app'
import MobileComicReadShell from './components/MobileComicReadShell.vue'
import MobileVideoPlay from './components/MobileVideoPlay.vue'
import MobileRemoteManage from './components/MobileRemoteManage.vue'
import { readerFullscreenActive } from './composables/useReaderFullscreen'
import { initLocalReadOnAppLaunch, localReadSession } from './localReadStore'
import { comicReadSubTab } from './comicReadUi'
import { comicStreamSession } from './comicStreamStore'
import { pickLocalReaderTitle } from './readerDisplayName'
import { videoStreamSession } from './videoStreamStore'
import { tryHandleAndroidBack } from './androidBackStack'

type SubNav = 'read' | 'video' | 'remote'

const subNav = ref<SubNav>('read')

const remoteBrowseDockFoot = computed(
  () => subNav.value === 'remote',
)

const showReaderTitleBar = computed(() => {
  if (subNav.value !== 'read' || readerFullscreenActive.value) return false
  if (comicReadSubTab.value !== 'home') return false
  if (!localReadSession.readingActive) return false
  const s = localReadSession
  return (
    pickLocalReaderTitle(
      s.readerTitle,
      s.folderSources,
      s.currentSourcePath,
      s.currentSourceIndex,
    ).length > 0
  )
})

const activeReaderTitle = computed(() => {
  const s = localReadSession
  return pickLocalReaderTitle(
    s.readerTitle,
    s.folderSources,
    s.currentSourcePath,
    s.currentSourceIndex,
  )
})

function openComicRead() {
  comicReadSubTab.value = 'home'
  subNav.value = 'read'
}

function openVideoPlay() {
  subNav.value = 'video'
}

function openRemoteManagement() {
  subNav.value = 'remote'
}

watch(
  () => videoStreamSession.navigateSeq,
  () => {
    openVideoPlay()
  },
)

watch(
  () => comicStreamSession.navigateSeq,
  () => {
    openComicRead()
    comicReadSubTab.value = 'home'
  },
)

let unlistenBack: (() => void) | null = null

onMounted(() => {
  initLocalReadOnAppLaunch()
  // 必須用 onBackButtonPress（plugin listener），邊緣滑動才會交給前端而非直接 finish Activity
  void onBackButtonPress(() => {
    if (tryHandleAndroidBack()) return
    // 遠端管理根層：回到漫畫閱讀，禁止邊緣手勢關 App
    if (subNav.value === 'remote') {
      subNav.value = 'read'
      return
    }
    // 其餘主分頁：吞掉邊緣／返回手勢，不結束 App（避免左右滑直接關掉）
  }).then((listener) => {
    unlistenBack = () => {
      void listener.unregister()
    }
  })
})

onBeforeUnmount(() => {
  unlistenBack?.()
  unlistenBack = null
})
</script>

<template>
  <div class="app">
    <div
      class="home home--nas-lite"
      :class="{ 'home--reader-fs': readerFullscreenActive }"
    >
      <div v-show="!readerFullscreenActive" class="home-header">
        <div class="sub-nav-row">
          <nav class="sub-nav">
            <button
              type="button"
              class="sub-link sub-link--tab"
              :class="{ on: subNav === 'read' }"
              @click.stop="openComicRead"
            >
              <span class="sub-link-text">漫畫閱讀</span>
            </button>
            <button
              type="button"
              class="sub-link sub-link--tab"
              :class="{ on: subNav === 'video' }"
              @click.stop="openVideoPlay"
            >
              <span class="sub-link-text">影片播放</span>
            </button>
            <button
              type="button"
              class="sub-link sub-link--tab"
              :class="{ on: subNav === 'remote' }"
              @click.stop="openRemoteManagement"
            >
              <span class="sub-link-text">遠端管理</span>
            </button>
          </nav>
        </div>

        <div v-if="showReaderTitleBar" class="reader-title-bar" :title="activeReaderTitle">
          {{ activeReaderTitle }}
        </div>
      </div>

      <div v-show="subNav === 'read'" class="comic-scroll read-panel">
        <MobileComicReadShell key="read-shell" class="read-mode-pane" />
      </div>

      <div v-show="subNav === 'video'" class="comic-scroll read-panel read-panel--video">
        <MobileVideoPlay key="video-play" class="read-mode-pane" />
      </div>

      <div v-show="subNav === 'remote'" class="comic-scroll remote-manage-scroll">
        <MobileRemoteManage :home-dock-active="remoteBrowseDockFoot" />
      </div>
    </div>

    <div
      v-show="!readerFullscreenActive && subNav === 'remote'"
      class="bottom-dock"
      @click.stop
    >
      <div id="gm-remote-browse-foot-slot" class="gm-remote-browse-foot-slot" />
    </div>
  </div>
</template>

<style scoped src="./app-shell.css"></style>
