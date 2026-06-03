<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import MobileComicReadShell from './components/MobileComicReadShell.vue'
import MobileVideoPlay from './components/MobileVideoPlay.vue'
import MobileRemoteManage from './components/MobileRemoteManage.vue'
import { readerFullscreenActive } from './composables/useReaderFullscreen'
import { initLocalReadOnAppLaunch, localReadSession } from './localReadStore'
import { comicReadSubTab } from './comicReadUi'
import { comicStreamSession } from './comicStreamStore'
import { pickLocalReaderTitle } from './readerDisplayName'
import { videoStreamSession } from './videoStreamStore'

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

onMounted(() => {
  initLocalReadOnAppLaunch()
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

      <div
        v-show="subNav === 'remote'"
        class="comic-scroll remote-manage-scroll"
        :class="{ 'remote-manage-scroll--browse': remoteBrowseDockFoot }"
      >
        <MobileRemoteManage />
      </div>
    </div>
  </div>
</template>

<style scoped src="./app-shell.css"></style>
