<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { defaultStreamFavoriteName } from '../streamFavoriteName'
import type {
  FavoriteJobGroup,
  FavoriteStreamAdapter,
  FavoriteStreamStateBase,
} from '../streamFavoriteTypes'
import type { RemoteStreamJob } from '../videoStreamStore'

const props = defineProps<{
  open: boolean
  adapter: FavoriteStreamAdapter<FavoriteStreamStateBase, RemoteStreamJob>
  /** 單一來源（串流列表加入等）；與 jobGroups 二擇一，jobGroups 優先 */
  jobs?: RemoteStreamJob[]
  /** 遠端多選：每個勾選根路徑一組，新增收藏時逐組命名 */
  jobGroups?: FavoriteJobGroup[]
  /** 預設收藏名稱建議（留空時自動用資料夾名） */
  defaultNameFallback?: string
}>()

const emit = defineEmits<{
  close: []
  done: [state: FavoriteStreamStateBase]
}>()

type Step = 'choose' | 'new' | 'existing'

const step = ref<Step>('choose')
const nameInput = ref('')
const categoryId = ref('')
const nameInputRef = ref<HTMLInputElement | null>(null)
const errorMsg = ref('')
/** 新增收藏時目前命名到第幾組（0-based） */
const newGroupIndex = ref(0)
/** 跳過命名，其餘（含目前）全部用資料夾名 */
const skipNamingAll = ref(false)

/** 不可用 computed(loadState)：無響應依賴會快取舊列表，剛新建的收藏不會出現 */
const state = ref<FavoriteStreamStateBase>(props.adapter.loadState())

function reloadState() {
  state.value = props.adapter.loadState()
}

const resolvedGroups = computed<FavoriteJobGroup[]>(() => {
  if (props.jobGroups && props.jobGroups.length > 0) {
    return props.jobGroups.filter((g) => g.jobs.length > 0)
  }
  const jobs = props.jobs ?? []
  if (jobs.length === 0) return []
  const defaultName = defaultStreamFavoriteName(
    jobs,
    props.defaultNameFallback ?? `收藏 ${jobs.length} 項`,
  )
  return [{ defaultName, jobs }]
})

const flatJobs = computed(() => resolvedGroups.value.flatMap((g) => g.jobs))
const totalJobCount = computed(() => flatJobs.value.length)
const newGroupCount = computed(() => resolvedGroups.value.length)
const currentNewGroup = computed(() => resolvedGroups.value[newGroupIndex.value] ?? null)
const currentSuggestedName = computed(
  () => currentNewGroup.value?.defaultName ?? props.defaultNameFallback ?? '收藏',
)

const playlistsInSelectedCategory = computed(() =>
  state.value.playlists.filter((p) => p.categoryId === categoryId.value),
)
const selectedCategoryName = computed(
  () => state.value.categories.find((c) => c.id === categoryId.value)?.name ?? '',
)

function ensureCategorySelected() {
  const cats = state.value.categories
  if (cats.length === 0) {
    categoryId.value = props.adapter.defaultCategoryId
    return
  }
  if (!cats.some((c) => c.id === categoryId.value)) {
    categoryId.value = cats.find((c) => c.id === props.adapter.defaultCategoryId)?.id ?? cats[0]!.id
  }
}

function resetToChoose() {
  reloadState()
  step.value = 'choose'
  errorMsg.value = ''
  newGroupIndex.value = 0
  skipNamingAll.value = false
  ensureCategorySelected()
}

watch(
  () => props.open,
  (isOpen) => {
    if (isOpen) {
      resetToChoose()
    }
  },
)

function onCreateCategory() {
  const name = window.prompt('新增主分類名稱')
  if (name === null || !name.trim()) return
  const beforeIds = new Set(state.value.categories.map((c) => c.id))
  const next = props.adapter.addCategory(name)
  state.value = next
  emit('done', next)
  const created = next.categories.find((c) => !beforeIds.has(c.id))
  categoryId.value = created?.id ?? next.categories[next.categories.length - 1]?.id ?? categoryId.value
  ensureCategorySelected()
}

function focusNameInput() {
  void nextTick(() => {
    nameInputRef.value?.focus()
    nameInputRef.value?.select()
  })
}

function openNewForm() {
  if (!categoryId.value) {
    errorMsg.value = '請先選擇主分類'
    return
  }
  if (resolvedGroups.value.length === 0) {
    errorMsg.value = '沒有可收藏的項目'
    return
  }
  step.value = 'new'
  errorMsg.value = ''
  newGroupIndex.value = 0
  skipNamingAll.value = false
  nameInput.value = resolvedGroups.value[0]!.defaultName
  focusNameInput()
}

function openExistingList() {
  if (!categoryId.value) {
    errorMsg.value = '請先選擇主分類'
    return
  }
  // 同頁剛新建的收藏也要立刻出現
  reloadState()
  step.value = 'existing'
  errorMsg.value = ''
}

function createPlaylistForGroup(group: FavoriteJobGroup, name: string) {
  const finalName = name.trim() || group.defaultName
  return props.adapter.addPlaylist(finalName, group.jobs, categoryId.value)
}

function confirmNew() {
  if (resolvedGroups.value.length === 0) {
    errorMsg.value = '沒有可收藏的項目'
    return
  }
  if (!categoryId.value) {
    errorMsg.value = '請先選擇主分類'
    return
  }

  let next: FavoriteStreamStateBase | null = null

  if (skipNamingAll.value) {
    // 自目前組起全部用資料夾名
    for (let i = newGroupIndex.value; i < resolvedGroups.value.length; i++) {
      const g = resolvedGroups.value[i]!
      next = createPlaylistForGroup(g, g.defaultName)
    }
    if (next) emit('done', next)
    emit('close')
    return
  }

  const group = currentNewGroup.value
  if (!group) {
    errorMsg.value = '沒有可收藏的項目'
    return
  }
  next = createPlaylistForGroup(group, nameInput.value)
  emit('done', next)

  const nextIndex = newGroupIndex.value + 1
  if (nextIndex >= resolvedGroups.value.length) {
    emit('close')
    return
  }
  newGroupIndex.value = nextIndex
  skipNamingAll.value = false
  nameInput.value = resolvedGroups.value[nextIndex]!.defaultName
  errorMsg.value = ''
  focusNameInput()
}

function pickExisting(playlistId: string) {
  if (flatJobs.value.length === 0) {
    errorMsg.value = '沒有可收藏的項目'
    return
  }
  const next = props.adapter.addJobsToPlaylist(playlistId, flatJobs.value)
  emit('done', next)
  emit('close')
}

function close() {
  emit('close')
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="fav-add-overlay" @click.self="close">
      <div class="fav-add-panel" @click.stop>
        <template v-if="step === 'choose'">
          <p class="fav-add-title">收藏（{{ totalJobCount }} 項）</p>
          <p v-if="newGroupCount > 1" class="fav-add-sub">
            已選 {{ newGroupCount }} 個來源；「新增收藏名稱」將逐一命名
          </p>
          <label class="fav-add-label" for="fav-add-cat-choose">主分類（必選）</label>
          <div class="fav-add-cat-row">
            <select id="fav-add-cat-choose" v-model="categoryId" class="fav-add-select fav-add-select--inline">
              <option v-for="c in state.categories" :key="c.id" :value="c.id">{{ c.name }}</option>
            </select>
            <button type="button" class="fav-add-btn fav-add-btn--tiny" @click="onCreateCategory">
              + 新建
            </button>
          </div>
          <div class="fav-add-actions fav-add-actions--col">
            <button type="button" class="fav-add-btn fav-add-btn--primary" @click="openNewForm">
              新增收藏名稱
            </button>
            <button type="button" class="fav-add-btn" @click="openExistingList">
              加入此分類下現有收藏
            </button>
            <button type="button" class="fav-add-btn fav-add-btn--ghost" @click="close">取消</button>
          </div>
          <p v-if="errorMsg" class="fav-add-error">{{ errorMsg }}</p>
        </template>

        <template v-else-if="step === 'new'">
          <p class="fav-add-title">
            新增收藏名稱
            <template v-if="newGroupCount > 1">
              （{{ newGroupIndex + 1 }}/{{ newGroupCount }}）
            </template>
          </p>
          <p class="fav-add-sub">主分類：{{ selectedCategoryName }}</p>
          <p v-if="currentNewGroup" class="fav-add-sub fav-add-sub--tight">
            來源：{{ currentNewGroup.defaultName }}（{{ currentNewGroup.jobs.length }} 項）
          </p>
          <label class="fav-add-label" for="fav-add-name">收藏名稱（可留空＝資料夾名）</label>
          <input
            id="fav-add-name"
            ref="nameInputRef"
            v-model="nameInput"
            class="fav-add-input"
            type="text"
            maxlength="80"
            autocomplete="off"
            :disabled="skipNamingAll"
            :placeholder="currentSuggestedName"
            @keydown.enter.prevent="confirmNew"
          />
          <label v-if="newGroupCount > 1" class="fav-add-check">
            <input v-model="skipNamingAll" type="checkbox" />
            <span>跳過命名，全部用資料夾名</span>
          </label>
          <p v-if="errorMsg" class="fav-add-error">{{ errorMsg }}</p>
          <div class="fav-add-actions">
            <button type="button" class="fav-add-btn fav-add-btn--primary" @click="confirmNew">
              {{
                skipNamingAll
                  ? '確認（其餘用資料夾名）'
                  : newGroupIndex + 1 < newGroupCount
                    ? '確認並命名下一個'
                    : '確認'
              }}
            </button>
            <button type="button" class="fav-add-btn fav-add-btn--ghost" @click="resetToChoose">返回</button>
          </div>
        </template>

        <template v-else>
          <p class="fav-add-title">加入現有收藏</p>
          <p class="fav-add-sub">主分類：{{ selectedCategoryName }}</p>
          <div v-if="playlistsInSelectedCategory.length === 0" class="fav-add-empty">
            此主分類下尚無收藏名稱，請改用「新增收藏名稱」
          </div>
          <div v-else class="fav-add-existing-scroll">
            <button
              v-for="pl in playlistsInSelectedCategory"
              :key="pl.id"
              type="button"
              class="fav-add-existing-item"
              @click="pickExisting(pl.id)"
            >
              <span class="fav-add-existing-name">{{ pl.name }}</span>
              <span class="fav-add-existing-count">{{ pl.jobs.length }}</span>
            </button>
          </div>
          <p v-if="errorMsg" class="fav-add-error">{{ errorMsg }}</p>
          <div class="fav-add-actions">
            <button type="button" class="fav-add-btn fav-add-btn--ghost" @click="resetToChoose">返回</button>
          </div>
        </template>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.fav-add-overlay {
  position: fixed;
  inset: 0;
  z-index: 120;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 20px;
}

.fav-add-panel {
  width: min(100%, 360px);
  max-height: 76vh;
  overflow-y: auto;
  padding: 18px;
  border-radius: 12px;
  background: #1a1a1a;
  border: 1px solid rgba(255, 255, 255, 0.15);
  box-sizing: border-box;
}

.fav-add-title {
  margin: 0 0 12px;
  font-weight: 600;
  font-size: 16px;
}

.fav-add-sub {
  margin: -6px 0 12px;
  font-size: 12px;
  opacity: 0.7;
}

.fav-add-sub--tight {
  margin-top: -8px;
}

.fav-add-label {
  display: block;
  margin: 0 0 6px;
  font-size: 12px;
  opacity: 0.75;
}

.fav-add-check {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: -4px 0 12px;
  font-size: 13px;
  opacity: 0.9;
  cursor: pointer;
  user-select: none;
}

.fav-add-check input {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}

.fav-add-cat-row {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 14px;
}

.fav-add-input,
.fav-add-select {
  width: 100%;
  box-sizing: border-box;
  margin: 0 0 14px;
  padding: 9px 11px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: #252525;
  color: inherit;
  font-size: 14px;
}

.fav-add-input:disabled {
  opacity: 0.55;
}

.fav-add-select--inline {
  margin: 0;
  flex: 1;
  min-width: 0;
}

.fav-add-error {
  margin: 0 0 10px;
  font-size: 12px;
  color: #ffb4b4;
}

.fav-add-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.fav-add-actions--col {
  flex-direction: column;
}

.fav-add-btn {
  flex: 1 1 auto;
  padding: 10px 14px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.2);
  background: transparent;
  color: inherit;
  font-size: 14px;
}

.fav-add-btn--primary {
  border: none;
  background: #3182ce;
  color: #fff;
  font-weight: 600;
}

.fav-add-btn--ghost {
  opacity: 0.85;
}

.fav-add-btn--tiny {
  flex: 0 0 auto;
  padding: 9px 10px;
  font-size: 12px;
}

.fav-add-empty {
  margin: 8px 0 14px;
  font-size: 13px;
  opacity: 0.75;
}

.fav-add-existing-scroll {
  max-height: 44vh;
  overflow-y: auto;
  margin-bottom: 12px;
}

.fav-add-existing-item {
  display: flex;
  align-items: center;
  gap: 8px;
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

.fav-add-existing-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fav-add-existing-count {
  flex-shrink: 0;
  font-size: 11px;
  opacity: 0.6;
}
</style>
