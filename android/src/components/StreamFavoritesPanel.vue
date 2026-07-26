<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { exportTextToPickedFolder, pickAndReadJsonText } from '../api'
import { formatInvokeError } from '../invokeError'
import {
  gridColsForStreamFavoriteLayout,
  isStreamFavoriteGridLayout,
  loadStreamFavoriteLayout,
  saveStreamFavoriteLayout,
  STREAM_FAVORITE_LAYOUT_OPTIONS,
  type StreamFavoriteLayout,
  type StreamFavoriteMedia,
} from '../streamFavoriteLayout'
import {
  buildStreamFavoriteExportJson,
  defaultStreamFavoriteExportFileName,
  parseStreamFavoriteImportText,
} from '../streamFavoriteTransfer'
import type {
  FavoriteStreamAdapter,
  FavoriteStreamPlaylistBase,
  FavoriteStreamStateBase,
} from '../streamFavoriteTypes'
import type { RemoteStreamJob } from '../videoStreamStore'
import { sortSelectedSlotsByName } from '../windowsNameSort'
import StreamFavoriteThumb from './StreamFavoriteThumb.vue'

const props = defineProps<{
  adapter: FavoriteStreamAdapter<FavoriteStreamStateBase, RemoteStreamJob>
  media: StreamFavoriteMedia
  currentKey?: string | null
  emptyHint?: string
  revision?: number
  progressForJob?: (job: RemoteStreamJob) => string | null
  progressRatioForJob?: (job: RemoteStreamJob) => number | null
  openedForJob?: (job: RemoteStreamJob) => boolean
}>()

const emit = defineEmits<{
  loadPlaylist: [jobs: RemoteStreamJob[], name: string]
  playItem: [job: RemoteStreamJob, sourceJobs: RemoteStreamJob[]]
}>()

const state = ref<FavoriteStreamStateBase>(props.adapter.loadState())
/** 主分類預設收合；僅明確展開的 id 會出現在此集合 */
const expandedCategoryIds = ref<Set<string>>(new Set())
const expandedPlaylistIds = ref<Set<string>>(new Set())
const layout = ref<StreamFavoriteLayout>(loadStreamFavoriteLayout(props.media))
const layoutMenuOpen = ref(false)
const opMenuOpen = ref(false)
const transferBusy = ref(false)
/** 重新排序：下次套用升序；每次成功後切換 */
const nameSortAscending = ref(true)

/** 選取鍵：c:<id> | p:<id> | i:<playlistId>\t<jobKey> */
const selectedKeys = ref<Set<string>>(new Set())

const moveItemDialog = ref<{ open: boolean; jobs: { playlistId: string; job: RemoteStreamJob }[] }>({
  open: false,
  jobs: [],
})
const movePlaylistDialog = ref<{ open: boolean; playlistIds: string[] }>({
  open: false,
  playlistIds: [],
})

const coverKind = computed(() => props.media)
const isGridLayout = computed(() => isStreamFavoriteGridLayout(layout.value))
const gridCols = computed(() => gridColsForStreamFavoriteLayout(layout.value))
const selectedCount = computed(() => selectedKeys.value.size)

const selectedCategories = computed(() =>
  state.value.categories.filter((c) => selectedKeys.value.has(catKey(c.id))),
)
const selectedPlaylists = computed(() =>
  state.value.playlists.filter((p) => selectedKeys.value.has(plKey(p.id))),
)
const selectedItems = computed(() => {
  const out: { playlistId: string; job: RemoteStreamJob }[] = []
  for (const pl of state.value.playlists) {
    for (const job of pl.jobs) {
      if (selectedKeys.value.has(itemKey(pl.id, props.adapter.jobKey(job)))) {
        out.push({ playlistId: pl.id, job })
      }
    }
  }
  return out
})

/**
 * 勾選連動會把子項一併勾上，因此「僅主分類／僅收藏名稱」幾乎永遠不成立。
 * 以下以「意圖」判斷：有勾主分類 → 主分類操作；否則有勾收藏名稱 → 收藏操作；否則項目操作。
 */
const intentCategories = computed(() => selectedCategories.value)
const intentPlaylists = computed(() =>
  intentCategories.value.length > 0 ? [] : selectedPlaylists.value,
)
const intentItems = computed(() => {
  if (intentCategories.value.length > 0 || selectedPlaylists.value.length > 0) return []
  return selectedItems.value
})

const canRename = computed(
  () =>
    intentCategories.value.length === 1 ||
    (intentCategories.value.length === 0 && intentPlaylists.value.length === 1),
)
const canDelete = computed(() => selectedCount.value > 0)
/** 收藏名稱（含連動子項）或純項目（含複選）皆可移動 */
const canMove = computed(
  () => intentPlaylists.value.length > 0 || intentItems.value.length > 0,
)
/** 至少勾選一個主分類或收藏名稱才可全展開／全收起（同時勾到影片項目不阻擋） */
const canExpandCollapse = computed(
  () => selectedCategories.value.length > 0 || selectedPlaylists.value.length > 0,
)

/**
 * 重新排序：任一層（主分類／收藏名稱／項目）至少勾 2 個即可；
 * 可同時勾多層，會分別對各層勾選項排序。
 */
const canSortByName = computed(() => {
  if (selectedCategories.value.length >= 2) return true
  if (selectedPlaylists.value.length >= 2) return true
  if (selectedItems.value.length >= 2) return true
  return false
})

/** 單獨意圖：一個主分類／一個收藏名稱／一個項目（允許連動勾到的子項） */
const singleReorderTarget = computed(() => {
  if (intentCategories.value.length === 1) {
    return { kind: 'category' as const, id: intentCategories.value[0]!.id }
  }
  if (intentPlaylists.value.length === 1) {
    return { kind: 'playlist' as const, id: intentPlaylists.value[0]!.id }
  }
  if (intentItems.value.length === 1) {
    const it = intentItems.value[0]!
    return { kind: 'item' as const, playlistId: it.playlistId, job: it.job }
  }
  return null
})

function reorderSiblingBounds(): { canUp: boolean; canDown: boolean } {
  const t = singleReorderTarget.value
  if (!t) return { canUp: false, canDown: false }
  if (t.kind === 'category') {
    const idx = state.value.categories.findIndex((c) => c.id === t.id)
    if (idx < 0) return { canUp: false, canDown: false }
    return { canUp: idx > 0, canDown: idx < state.value.categories.length - 1 }
  }
  if (t.kind === 'playlist') {
    const pl = state.value.playlists.find((p) => p.id === t.id)
    if (!pl) return { canUp: false, canDown: false }
    const siblings = state.value.playlists.filter((p) => p.categoryId === pl.categoryId)
    const idx = siblings.findIndex((p) => p.id === t.id)
    if (idx < 0) return { canUp: false, canDown: false }
    return { canUp: idx > 0, canDown: idx < siblings.length - 1 }
  }
  const pl = state.value.playlists.find((p) => p.id === t.playlistId)
  if (!pl) return { canUp: false, canDown: false }
  const key = props.adapter.jobKey(t.job)
  const idx = pl.jobs.findIndex((j) => props.adapter.jobKey(j) === key)
  if (idx < 0) return { canUp: false, canDown: false }
  return { canUp: idx > 0, canDown: idx < pl.jobs.length - 1 }
}

const canMoveUp = computed(() => reorderSiblingBounds().canUp)
const canMoveDown = computed(() => reorderSiblingBounds().canDown)

function catKey(id: string) {
  return `c:${id}`
}
function plKey(id: string) {
  return `p:${id}`
}
function itemKey(playlistId: string, jobKey: string) {
  return `i:${playlistId}\t${jobKey}`
}

function reload() {
  state.value = props.adapter.loadState()
  // 清掉已不存在的選取
  const valid = new Set<string>()
  for (const c of state.value.categories) valid.add(catKey(c.id))
  for (const p of state.value.playlists) {
    valid.add(plKey(p.id))
    for (const j of p.jobs) valid.add(itemKey(p.id, props.adapter.jobKey(j)))
  }
  selectedKeys.value = new Set([...selectedKeys.value].filter((k) => valid.has(k)))
}

watch(
  () => props.revision,
  () => reload(),
)

onMounted(() => reload())

function playlistsInCategory(categoryId: string): FavoriteStreamPlaylistBase[] {
  return state.value.playlists.filter((p) => p.categoryId === categoryId)
}

function isCategoryExpanded(id: string): boolean {
  return expandedCategoryIds.value.has(id)
}

function toggleCategory(id: string) {
  const next = new Set(expandedCategoryIds.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expandedCategoryIds.value = next
}

function isPlaylistExpanded(id: string): boolean {
  return expandedPlaylistIds.value.has(id)
}

function togglePlaylist(id: string) {
  const next = new Set(expandedPlaylistIds.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expandedPlaylistIds.value = next
}

function isSelected(key: string) {
  return selectedKeys.value.has(key)
}

function addPlaylistSubtree(next: Set<string>, playlistId: string) {
  const pl = state.value.playlists.find((p) => p.id === playlistId)
  if (!pl) return
  next.add(plKey(pl.id))
  for (const job of pl.jobs) {
    next.add(itemKey(pl.id, props.adapter.jobKey(job)))
  }
}

function removePlaylistSubtree(next: Set<string>, playlistId: string) {
  const pl = state.value.playlists.find((p) => p.id === playlistId)
  if (!pl) {
    next.delete(plKey(playlistId))
    return
  }
  next.delete(plKey(pl.id))
  for (const job of pl.jobs) {
    next.delete(itemKey(pl.id, props.adapter.jobKey(job)))
  }
}

function addCategorySubtree(next: Set<string>, categoryId: string) {
  next.add(catKey(categoryId))
  for (const pl of playlistsInCategory(categoryId)) {
    addPlaylistSubtree(next, pl.id)
  }
}

function removeCategorySubtree(next: Set<string>, categoryId: string) {
  next.delete(catKey(categoryId))
  for (const pl of playlistsInCategory(categoryId)) {
    removePlaylistSubtree(next, pl.id)
  }
}

/** 子項全選 → 勾上層；否則取消上層（避免全選後只取消可見列仍殘留數百個隱藏勾選） */
function syncAncestorsFromLeaves(next: Set<string>) {
  for (const pl of state.value.playlists) {
    if (pl.jobs.length === 0) continue
    const allItems = pl.jobs.every((j) => next.has(itemKey(pl.id, props.adapter.jobKey(j))))
    if (allItems) next.add(plKey(pl.id))
    else next.delete(plKey(pl.id))
  }
  for (const cat of state.value.categories) {
    const pls = playlistsInCategory(cat.id)
    if (pls.length === 0) continue
    const allPls = pls.every((p) => {
      if (!next.has(plKey(p.id))) return false
      if (p.jobs.length === 0) return true
      return p.jobs.every((j) => next.has(itemKey(p.id, props.adapter.jobKey(j))))
    })
    if (allPls) next.add(catKey(cat.id))
    else next.delete(catKey(cat.id))
  }
}

function toggleSelected(key: string) {
  const next = new Set(selectedKeys.value)
  const selecting = !next.has(key)

  if (key.startsWith('c:')) {
    const catId = key.slice(2)
    if (selecting) addCategorySubtree(next, catId)
    else removeCategorySubtree(next, catId)
  } else if (key.startsWith('p:')) {
    const plId = key.slice(2)
    if (selecting) addPlaylistSubtree(next, plId)
    else removePlaylistSubtree(next, plId)
    syncAncestorsFromLeaves(next)
  } else if (key.startsWith('i:')) {
    if (selecting) next.add(key)
    else next.delete(key)
    syncAncestorsFromLeaves(next)
  } else {
    if (selecting) next.add(key)
    else next.delete(key)
  }

  selectedKeys.value = next
}

function closeMenus() {
  layoutMenuOpen.value = false
  opMenuOpen.value = false
}

function toggleLayoutMenu() {
  const next = !layoutMenuOpen.value
  closeMenus()
  layoutMenuOpen.value = next
}

function toggleOpMenu() {
  const next = !opMenuOpen.value
  closeMenus()
  opMenuOpen.value = next
}

function chooseLayout(next: StreamFavoriteLayout) {
  layout.value = next
  saveStreamFavoriteLayout(props.media, next)
  layoutMenuOpen.value = false
}

function runAddCategory() {
  opMenuOpen.value = false
  const name = window.prompt('新增主分類名稱')
  if (name === null || !name.trim()) return
  state.value = props.adapter.addCategory(name)
}

async function runExportFavorites() {
  opMenuOpen.value = false
  if (transferBusy.value) return
  transferBusy.value = true
  try {
    const fileName = defaultStreamFavoriteExportFileName(props.media)
    const content = buildStreamFavoriteExportJson(props.media, props.adapter.loadState())
    const uri = await exportTextToPickedFolder(fileName, content)
    if (!uri) return
    window.alert(`已導出收藏\n${fileName}`)
  } catch (e) {
    window.alert(formatInvokeError(e))
  } finally {
    transferBusy.value = false
  }
}

async function runImportFavorites() {
  opMenuOpen.value = false
  if (transferBusy.value) return
  if (
    !window.confirm(
      '導入收藏將覆蓋目前此頁全部串流收藏（主分類／收藏名稱／項目），確定繼續？',
    )
  ) {
    return
  }
  transferBusy.value = true
  try {
    const text = await pickAndReadJsonText()
    if (text === null) return
    const imported = parseStreamFavoriteImportText(text, props.media)
    state.value = props.adapter.replaceState(imported as FavoriteStreamStateBase)
    selectedKeys.value = new Set()
    reload()
    window.alert('已導入並覆蓋現有收藏')
  } catch (e) {
    window.alert(formatInvokeError(e))
  } finally {
    transferBusy.value = false
  }
}

function allSelectableKeys(): string[] {
  const keys: string[] = []
  for (const c of state.value.categories) keys.push(catKey(c.id))
  for (const p of state.value.playlists) {
    keys.push(plKey(p.id))
    for (const j of p.jobs) keys.push(itemKey(p.id, props.adapter.jobKey(j)))
  }
  return keys
}

const allSelected = computed(() => {
  const keys = allSelectableKeys()
  return keys.length > 0 && keys.every((k) => selectedKeys.value.has(k))
})

function toggleSelectAll() {
  if (allSelected.value) {
    selectedKeys.value = new Set()
    return
  }
  selectedKeys.value = new Set(allSelectableKeys())
}

function runRename() {
  opMenuOpen.value = false
  if (!canRename.value) return
  if (intentCategories.value.length === 1) {
    const cat = intentCategories.value[0]!
    const name = window.prompt('重新命名主分類', cat.name)
    if (name === null) return
    state.value = props.adapter.renameCategory(cat.id, name)
    return
  }
  if (intentPlaylists.value.length === 1) {
    const pl = intentPlaylists.value[0]!
    const name = window.prompt('重新命名收藏', pl.name)
    if (name === null) return
    state.value = props.adapter.renamePlaylist(pl.id, name)
  }
}

function runDelete() {
  opMenuOpen.value = false
  if (!canDelete.value) return
  const parts: string[] = []
  if (selectedCategories.value.length) parts.push(`${selectedCategories.value.length} 個主分類`)
  if (selectedPlaylists.value.length) parts.push(`${selectedPlaylists.value.length} 個收藏`)
  if (selectedItems.value.length) parts.push(`${selectedItems.value.length} 個項目`)
  if (!window.confirm(`確定刪除：${parts.join('、')}？`)) return

  // 先刪項目／收藏，再刪主分類（主分類刪除會把收藏移入未分類）
  for (const { playlistId, job } of selectedItems.value) {
    state.value = props.adapter.removePlaylistItem(playlistId, job)
  }
  for (const pl of selectedPlaylists.value) {
    state.value = props.adapter.removePlaylist(pl.id)
  }
  for (const cat of selectedCategories.value) {
    try {
      state.value = props.adapter.removeCategory(cat.id)
    } catch (e) {
      window.alert(e instanceof Error ? e.message : String(e))
    }
  }
  selectedKeys.value = new Set()
  reload()
}

function runMove() {
  opMenuOpen.value = false
  if (!canMove.value) return
  if (intentPlaylists.value.length > 0) {
    movePlaylistDialog.value = {
      open: true,
      playlistIds: intentPlaylists.value.map((p) => p.id),
    }
    return
  }
  if (intentItems.value.length > 0) {
    moveItemDialog.value = { open: true, jobs: [...intentItems.value] }
  }
}

function runReorder(direction: -1 | 1) {
  // 上移／下移保持操作選單開啟，方便連續調整
  const t = singleReorderTarget.value
  if (!t) return
  if (direction < 0 && !canMoveUp.value) return
  if (direction > 0 && !canMoveDown.value) return
  if (t.kind === 'category') {
    state.value = props.adapter.reorderCategory(t.id, direction)
  } else if (t.kind === 'playlist') {
    state.value = props.adapter.reorderPlaylist(t.id, direction)
  } else {
    state.value = props.adapter.reorderPlaylistItem(t.playlistId, t.job, direction)
  }
  reload()
}

function closeMovePlaylistDialog() {
  movePlaylistDialog.value = { open: false, playlistIds: [] }
}

function closeMoveItemDialog() {
  moveItemDialog.value = { open: false, jobs: [] }
}

const movablePlaylistCategories = computed(() => {
  const ids = new Set(
    movePlaylistDialog.value.playlistIds
      .map((id) => state.value.playlists.find((p) => p.id === id)?.categoryId)
      .filter(Boolean),
  )
  // 若所有選中收藏已在同一分類，仍可移到其他分類；顯示全部其他分類
  if (ids.size === 1) {
    const only = [...ids][0]!
    return state.value.categories.filter((c) => c.id !== only)
  }
  return state.value.categories
})

function confirmMovePlaylist(categoryId: string) {
  for (const id of movePlaylistDialog.value.playlistIds) {
    state.value = props.adapter.movePlaylistToCategory(id, categoryId)
  }
  selectedKeys.value = new Set()
  closeMovePlaylistDialog()
  reload()
}

function movableTargetPlaylists(categoryId: string): FavoriteStreamPlaylistBase[] {
  const sourcePlIds = new Set(moveItemDialog.value.jobs.map((j) => j.playlistId))
  return state.value.playlists.filter((p) => p.categoryId === categoryId && !sourcePlIds.has(p.id))
}

const hasMovableTargets = computed(() =>
  state.value.playlists.some((p) => !moveItemDialog.value.jobs.every((j) => j.playlistId === p.id)),
)

function confirmMoveItem(targetPlaylistId: string) {
  for (const { playlistId, job } of moveItemDialog.value.jobs) {
    if (playlistId === targetPlaylistId) continue
    state.value = props.adapter.moveItemToPlaylist(playlistId, targetPlaylistId, job)
  }
  selectedKeys.value = new Set()
  closeMoveItemDialog()
  reload()
}

function onPlaylistNameClick(pl: FavoriteStreamPlaylistBase) {
  if (pl.jobs.length === 0) return
  emit('loadPlaylist', pl.jobs, pl.name)
}

function onItemClick(pl: FavoriteStreamPlaylistBase, job: RemoteStreamJob) {
  emit('playItem', job, pl.jobs)
}

function runExpandSelected() {
  if (!canExpandCollapse.value) return
  const nextCats = new Set(expandedCategoryIds.value)
  const nextExpanded = new Set(expandedPlaylistIds.value)

  for (const cat of selectedCategories.value) {
    nextCats.add(cat.id)
    for (const pl of playlistsInCategory(cat.id)) {
      nextExpanded.add(pl.id)
    }
  }
  for (const pl of selectedPlaylists.value) {
    nextExpanded.add(pl.id)
    nextCats.add(pl.categoryId)
  }

  expandedCategoryIds.value = nextCats
  expandedPlaylistIds.value = nextExpanded
}

function runCollapseSelected() {
  if (!canExpandCollapse.value) return
  const nextCats = new Set(expandedCategoryIds.value)
  const nextExpanded = new Set(expandedPlaylistIds.value)

  for (const cat of selectedCategories.value) {
    nextCats.delete(cat.id)
    for (const pl of playlistsInCategory(cat.id)) {
      nextExpanded.delete(pl.id)
    }
  }
  for (const pl of selectedPlaylists.value) {
    nextExpanded.delete(pl.id)
  }

  expandedCategoryIds.value = nextCats
  expandedPlaylistIds.value = nextExpanded
}

function persistFavoriteState(next: FavoriteStreamStateBase) {
  state.value = props.adapter.replaceState(next)
  reload()
}

function runSortByName() {
  opMenuOpen.value = false
  layoutMenuOpen.value = false
  if (!canSortByName.value) return
  const ascending = nameSortAscending.value
  const selectedCatIds = new Set(selectedCategories.value.map((c) => c.id))
  const selectedPlIds = new Set(selectedPlaylists.value.map((p) => p.id))
  const selectedItemKeys = new Set(
    selectedItems.value.map((it) => itemKey(it.playlistId, props.adapter.jobKey(it.job))),
  )

  let nextCategories = state.value.categories
  let nextPlaylists = state.value.playlists
  let changed = false

  if (selectedCatIds.size >= 2) {
    nextCategories = sortSelectedSlotsByName(
      nextCategories,
      (c) => selectedCatIds.has(c.id),
      (c) => c.name,
      ascending,
    )
    changed = true
  }

  if (selectedPlIds.size >= 2) {
    const rebuilt: typeof state.value.playlists = []
    const seen = new Set<string>()
    for (const cat of nextCategories) {
      const siblings = nextPlaylists.filter((p) => p.categoryId === cat.id)
      const sorted = sortSelectedSlotsByName(
        siblings,
        (p) => selectedPlIds.has(p.id),
        (p) => p.name,
        ascending,
      )
      for (const p of sorted) {
        rebuilt.push(p)
        seen.add(p.id)
      }
    }
    for (const p of nextPlaylists) {
      if (!seen.has(p.id)) rebuilt.push(p)
    }
    nextPlaylists = rebuilt
    changed = true
  }

  if (selectedItemKeys.size >= 2) {
    nextPlaylists = nextPlaylists.map((pl) => {
      const jobs = sortSelectedSlotsByName(
        pl.jobs,
        (job) => selectedItemKeys.has(itemKey(pl.id, props.adapter.jobKey(job))),
        (job) => job.title || job.relPath,
        ascending,
      )
      if (jobs === pl.jobs) return pl
      return { ...pl, jobs }
    })
    changed = true
  }

  if (!changed) return

  persistFavoriteState({
    categories: nextCategories,
    playlists: nextPlaylists,
  })
  nameSortAscending.value = !ascending
}
</script>

<template>
  <div class="sfp-root">
    <div class="sfp-toolbar">
      <div class="sfp-toolbar-left">
        <button type="button" class="sfp-btn sfp-btn--ghost" @click="toggleSelectAll">
          {{ allSelected ? '取消全選' : '全選' }}
        </button>
        <div class="sfp-menu-wrap">
          <button type="button" class="sfp-btn sfp-btn--ghost" @click.stop="toggleOpMenu">
            操作{{ selectedCount > 0 ? ` (${selectedCount})` : '' }} ▾
          </button>
          <div v-if="opMenuOpen" class="sfp-menu" @click.stop>
            <button type="button" @click="runAddCategory">新增主分類</button>
            <button type="button" :disabled="!canRename" @click="runRename">改名</button>
            <button type="button" :disabled="!canMove" @click="runMove">移動</button>
            <button type="button" :disabled="!canMoveUp" @click="runReorder(-1)">上移</button>
            <button type="button" :disabled="!canMoveDown" @click="runReorder(1)">下移</button>
            <button type="button" :disabled="!canExpandCollapse" @click="runExpandSelected">全展開</button>
            <button type="button" :disabled="!canExpandCollapse" @click="runCollapseSelected">全收起</button>
            <button type="button" :disabled="transferBusy" @click="runExportFavorites">導出收藏</button>
            <button type="button" :disabled="transferBusy" @click="runImportFavorites">導入收藏</button>
            <button type="button" class="sfp-menu-danger" :disabled="!canDelete" @click="runDelete">
              刪除
            </button>
          </div>
        </div>
      </div>
      <div class="sfp-toolbar-right">
        <button
          type="button"
          class="sfp-btn sfp-btn--ghost"
          :disabled="!canSortByName"
          :title="canSortByName ? '依名稱重新排序所選項目' : '請至少勾選同層兩項'"
          @click="runSortByName"
        >
          {{ nameSortAscending ? '升序' : '降序' }}
        </button>
        <div class="sfp-menu-wrap">
          <button
            type="button"
            class="sfp-btn sfp-btn--ghost"
            :aria-expanded="layoutMenuOpen"
            @click.stop="toggleLayoutMenu"
          >
            顯示方式 ▾
          </button>
          <div v-if="layoutMenuOpen" class="sfp-menu sfp-menu--right" @click.stop>
            <button
              v-for="opt in STREAM_FAVORITE_LAYOUT_OPTIONS"
              :key="opt.key"
              type="button"
              class="sfp-menu-item"
              :class="{ on: layout === opt.key }"
              @click="chooseLayout(opt.key)"
            >
              {{ opt.label }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="sfp-scroll" @click="closeMenus">
      <p v-if="state.playlists.length === 0 && state.categories.length <= 1" class="sfp-empty">
        {{ emptyHint ?? '尚無收藏' }}
      </p>
      <div v-for="cat in state.categories" :key="cat.id" class="sfp-cat-block">
        <div class="sfp-cat-head" :class="{ 'sfp-row--selected': isSelected(catKey(cat.id)) }">
          <button
            type="button"
            class="sfp-check-btn"
            :aria-pressed="isSelected(catKey(cat.id))"
            @click.stop="toggleSelected(catKey(cat.id))"
          >
            <span class="sfp-check">{{ isSelected(catKey(cat.id)) ? '☑' : '☐' }}</span>
          </button>
          <span class="sfp-cat-name">{{ cat.name }}</span>
          <span class="sfp-count">{{ playlistsInCategory(cat.id).length }}</span>
          <button
            type="button"
            class="sfp-toggle"
            :title="isCategoryExpanded(cat.id) ? '收合' : '展開'"
            @click="toggleCategory(cat.id)"
          >
            {{ isCategoryExpanded(cat.id) ? '▼' : '▶' }}
          </button>
        </div>

        <div v-if="isCategoryExpanded(cat.id)" class="sfp-cat-body">
          <p v-if="playlistsInCategory(cat.id).length === 0" class="sfp-empty-sub">（此分類尚無收藏）</p>
          <div v-for="pl in playlistsInCategory(cat.id)" :key="pl.id" class="sfp-pl-block">
            <div class="sfp-pl-head" :class="{ 'sfp-row--selected': isSelected(plKey(pl.id)) }">
              <button
                type="button"
                class="sfp-check-btn"
                :aria-pressed="isSelected(plKey(pl.id))"
                @click.stop="toggleSelected(plKey(pl.id))"
              >
                <span class="sfp-check">{{ isSelected(plKey(pl.id)) ? '☑' : '☐' }}</span>
              </button>
              <button type="button" class="sfp-pl-name" @click="onPlaylistNameClick(pl)">{{ pl.name }}</button>
              <span class="sfp-count">{{ pl.jobs.length }} {{ adapter.unitLabel }}</span>
              <button
                type="button"
                class="sfp-toggle"
                :title="isPlaylistExpanded(pl.id) ? '收合' : '展開'"
                @click="togglePlaylist(pl.id)"
              >
                {{ isPlaylistExpanded(pl.id) ? '▼' : '▶' }}
              </button>
            </div>

            <div v-if="isPlaylistExpanded(pl.id)">
              <p v-if="pl.jobs.length === 0" class="sfp-empty-sub">（空）</p>
              <ul v-else-if="!isGridLayout" class="sfp-item-list">
                <li
                  v-for="job in pl.jobs"
                  :key="adapter.jobKey(job)"
                  class="sfp-item-row"
                  :class="{
                    'sfp-item-row--current': currentKey != null && adapter.jobKey(job) === currentKey,
                    'sfp-row--selected': isSelected(itemKey(pl.id, adapter.jobKey(job))),
                  }"
                >
                  <button
                    type="button"
                    class="sfp-check-btn"
                    :aria-pressed="isSelected(itemKey(pl.id, adapter.jobKey(job)))"
                    @click.stop="toggleSelected(itemKey(pl.id, adapter.jobKey(job)))"
                  >
                    <span class="sfp-check">{{
                      isSelected(itemKey(pl.id, adapter.jobKey(job))) ? '☑' : '☐'
                    }}</span>
                  </button>
                  <button type="button" class="sfp-item-main" @click="onItemClick(pl, job)">
                    <span class="sfp-item-title-row">
                      <span v-if="openedForJob?.(job)" class="sfp-item-opened" title="已開啟過">●</span>
                      <span class="sfp-item-title">{{ job.title }}</span>
                    </span>
                    <span class="sfp-item-meta">{{ job.relPath }}</span>
                    <span v-if="progressForJob?.(job)" class="sfp-item-progress">{{ progressForJob(job) }}</span>
                    <div
                      v-if="progressRatioForJob?.(job) != null"
                      class="sfp-item-timeline"
                      aria-hidden="true"
                    >
                      <div
                        class="sfp-item-timeline-fill"
                        :style="{ width: `${Math.round((progressRatioForJob(job) ?? 0) * 100)}%` }"
                      />
                    </div>
                  </button>
                </li>
              </ul>
              <div
                v-else
                class="sfp-item-grid"
                :style="{ gridTemplateColumns: `repeat(${gridCols}, minmax(0, 1fr))` }"
              >
                <div
                  v-for="job in pl.jobs"
                  :key="adapter.jobKey(job)"
                  class="sfp-item-grid-cell"
                  :class="{
                    'sfp-item-grid-cell--current': currentKey != null && adapter.jobKey(job) === currentKey,
                    'sfp-row--selected': isSelected(itemKey(pl.id, adapter.jobKey(job))),
                  }"
                >
                  <button
                    type="button"
                    class="sfp-grid-check-wrap"
                    :aria-pressed="isSelected(itemKey(pl.id, adapter.jobKey(job)))"
                    @click.stop="toggleSelected(itemKey(pl.id, adapter.jobKey(job)))"
                  >
                    <span class="sfp-check">{{
                      isSelected(itemKey(pl.id, adapter.jobKey(job))) ? '☑' : '☐'
                    }}</span>
                  </button>
                  <button type="button" class="sfp-item-grid-main" @click="onItemClick(pl, job)">
                    <StreamFavoriteThumb :job="job" :kind="coverKind" />
                    <span class="sfp-item-grid-name" :title="job.title">{{ job.title }}</span>
                    <span v-if="progressForJob?.(job)" class="sfp-item-grid-progress">{{
                      progressForJob(job)
                    }}</span>
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <Teleport to="body">
      <div v-if="movePlaylistDialog.open" class="sfp-move-overlay" @click.self="closeMovePlaylistDialog">
        <div class="sfp-move-panel">
          <p class="sfp-move-title">移動收藏到主分類（{{ movePlaylistDialog.playlistIds.length }}）</p>
          <div class="sfp-move-scroll">
            <button
              v-for="cat in movablePlaylistCategories"
              :key="cat.id"
              type="button"
              class="sfp-move-item"
              @click="confirmMovePlaylist(cat.id)"
            >
              {{ cat.name }}
            </button>
            <p v-if="movablePlaylistCategories.length === 0" class="sfp-move-empty">沒有其他主分類</p>
          </div>
          <button type="button" class="sfp-move-cancel" @click="closeMovePlaylistDialog">取消</button>
        </div>
      </div>

      <div v-if="moveItemDialog.open" class="sfp-move-overlay" @click.self="closeMoveItemDialog">
        <div class="sfp-move-panel">
          <p class="sfp-move-title">移動項目到收藏（{{ moveItemDialog.jobs.length }}）</p>
          <div class="sfp-move-scroll">
            <template v-for="cat in state.categories" :key="cat.id">
              <template v-if="movableTargetPlaylists(cat.id).length > 0">
                <p class="sfp-move-cat-heading">{{ cat.name }}</p>
                <button
                  v-for="pl in movableTargetPlaylists(cat.id)"
                  :key="pl.id"
                  type="button"
                  class="sfp-move-item"
                  @click="confirmMoveItem(pl.id)"
                >
                  {{ pl.name }}
                </button>
              </template>
            </template>
            <p v-if="!hasMovableTargets" class="sfp-move-empty">沒有其他收藏名稱可移動</p>
          </div>
          <button type="button" class="sfp-move-cancel" @click="closeMoveItemDialog">取消</button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.sfp-root {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  width: 100%;
}

.sfp-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-shrink: 0;
  margin: 0 0 6px;
  padding: 0;
  position: relative;
  z-index: 30;
}

.sfp-toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.sfp-toolbar-left {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  min-width: 0;
}

.sfp-btn {
  padding: 6px 10px;
  border-radius: 8px;
  font-size: 12px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: transparent;
  color: inherit;
}

.sfp-btn--ghost {
  opacity: 0.9;
}

.sfp-btn--foot {
  padding: 5px 10px;
  white-space: nowrap;
}

.sfp-btn:disabled {
  opacity: 0.4;
}

.sfp-menu-wrap {
  position: relative;
}

.sfp-menu {
  position: absolute;
  left: 0;
  top: calc(100% + 6px);
  z-index: 80;
  min-width: 148px;
  padding: 4px 0;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.18);
  background: #2a2a2a;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.45);
  display: flex;
  flex-direction: column;
}

.sfp-menu--right {
  left: auto;
  right: 0;
}

.sfp-menu > button,
.sfp-menu-item {
  width: 100%;
  padding: 9px 14px;
  border: none;
  background: transparent;
  color: #eee;
  font-size: 13px;
  text-align: left;
}

.sfp-menu > button:disabled {
  opacity: 0.35;
}

.sfp-menu > button:not(:disabled):hover,
.sfp-menu-item:hover,
.sfp-menu-item.on {
  background: rgba(49, 130, 206, 0.22);
}

.sfp-menu-item.on {
  color: #9fd0ff;
}

.sfp-menu-danger:not(:disabled) {
  color: #ffb4b4;
}

.sfp-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.sfp-empty,
.sfp-empty-sub {
  margin: 8px 0;
  font-size: 12px;
  opacity: 0.7;
}

.sfp-cat-block {
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 10px;
  padding: 8px;
}

.sfp-cat-head,
.sfp-pl-head {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: nowrap;
  border-radius: 6px;
  padding: 2px 4px;
}

.sfp-row--selected {
  background: rgba(49, 130, 206, 0.14);
}

.sfp-check-btn {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 36px;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
}

.sfp-check {
  flex-shrink: 0;
  width: 1.2em;
  text-align: center;
  opacity: 0.9;
  font-size: 16px;
  line-height: 1;
  user-select: none;
}

.sfp-cat-body {
  margin-top: 6px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.sfp-toggle {
  border: none;
  background: transparent;
  color: #9ab;
  padding: 4px 8px;
  flex-shrink: 0;
  font-size: 12px;
  margin-left: 2px;
}

.sfp-cat-name {
  flex: 1;
  min-width: 0;
  font-weight: 700;
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sfp-pl-block {
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 10px;
  padding: 8px;
  margin-left: 4px;
}

.sfp-pl-name {
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

.sfp-count {
  flex-shrink: 0;
  font-size: 11px;
  opacity: 0.65;
}

.sfp-item-list {
  list-style: none;
  margin: 6px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.sfp-item-row {
  display: flex;
  align-items: stretch;
  gap: 6px;
  border-radius: 8px;
  padding: 2px;
}

.sfp-item-row--current .sfp-item-main {
  border-color: rgba(49, 130, 206, 0.65);
  background: rgba(49, 130, 206, 0.15);
}

.sfp-item-main {
  flex: 1;
  min-width: 0;
  text-align: left;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(0, 0, 0, 0.2);
  color: inherit;
}

.sfp-item-title-row {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}

.sfp-item-opened {
  flex-shrink: 0;
  color: #6eb5ff;
  font-size: 10px;
}

.sfp-item-title {
  display: block;
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}

.sfp-item-meta {
  display: block;
  font-size: 10px;
  opacity: 0.65;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 2px;
}

.sfp-item-progress {
  display: block;
  font-size: 11px;
  color: #9ab;
  margin-top: 2px;
}

.sfp-item-timeline {
  margin-top: 5px;
  height: 3px;
  border-radius: 2px;
  background: rgba(255, 255, 255, 0.1);
  overflow: hidden;
}

.sfp-item-timeline-fill {
  height: 100%;
  border-radius: inherit;
  background: linear-gradient(90deg, #3d6ef5, #6eb5ff);
  min-width: 2px;
}

.sfp-item-grid {
  display: grid;
  gap: 8px 6px;
  margin-top: 6px;
}

.sfp-item-grid-cell {
  position: relative;
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 6px;
  overflow: hidden;
  background: rgba(0, 0, 0, 0.15);
}

.sfp-item-grid-cell--current {
  border-color: rgba(49, 130, 206, 0.65);
  background: rgba(49, 130, 206, 0.12);
}

.sfp-grid-check-wrap {
  position: absolute;
  top: 4px;
  left: 4px;
  z-index: 2;
  padding: 2px 4px;
  border: none;
  background: rgba(0, 0, 0, 0.45);
  border-radius: 4px;
  color: inherit;
}

.sfp-item-grid-main {
  display: flex;
  flex-direction: column;
  width: 100%;
  padding: 6px 5px 4px;
  border: none;
  background: transparent;
  color: inherit;
  text-align: center;
}

.sfp-item-grid-name {
  margin-top: 4px;
  font-size: 10px;
  line-height: 1.25;
  max-height: 2.4em;
  overflow: hidden;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  word-break: break-all;
}

.sfp-item-grid-progress {
  margin-top: 2px;
  font-size: 9px;
  opacity: 0.65;
}

.sfp-move-overlay {
  position: fixed;
  inset: 0;
  z-index: 130;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}

.sfp-move-panel {
  width: min(100%, 340px);
  max-height: 70vh;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  padding: 16px;
  border-radius: 12px;
  background: #1a1a1a;
  border: 1px solid rgba(255, 255, 255, 0.15);
}

.sfp-move-title {
  margin: 0 0 10px;
  font-weight: 600;
  font-size: 15px;
}

.sfp-move-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  margin-bottom: 10px;
}

.sfp-move-cat-heading {
  margin: 10px 0 4px;
  font-size: 11px;
  opacity: 0.6;
}

.sfp-move-item {
  display: block;
  width: 100%;
  box-sizing: border-box;
  margin-bottom: 6px;
  padding: 9px 11px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(255, 255, 255, 0.04);
  color: inherit;
  text-align: left;
  font-size: 13px;
}

.sfp-move-empty {
  margin: 8px 0;
  font-size: 12px;
  opacity: 0.7;
}

.sfp-move-cancel {
  flex-shrink: 0;
  width: 100%;
  padding: 9px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: transparent;
  color: inherit;
}
</style>
