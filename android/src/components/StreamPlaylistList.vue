<script setup lang="ts">
import { computed } from 'vue'

export type StreamListJob = {
  host: string
  port: number
  relPath: string
  title: string
}

const props = defineProps<{
  jobs: StreamListJob[]
  currentKey?: string | null
  emptyText?: string
  showRemove?: boolean
  fill?: boolean
  /** 每列閱讀進度（例：5/120頁） */
  progressForJob?: (job: StreamListJob) => string | null
  /** 每列是否顯示已開啟標記 */
  openedForJob?: (job: StreamListJob) => boolean
}>()

const emit = defineEmits<{
  play: [job: StreamListJob]
  remove: [job: StreamListJob]
}>()

function jobKey(job: StreamListJob): string {
  return `${job.host.trim().toLowerCase()}:${job.port}:${job.relPath}`
}

const rows = computed(() =>
  props.jobs.map((job) => ({
    job,
    key: jobKey(job),
    playing: props.currentKey != null && jobKey(job) === props.currentKey,
  })),
)
</script>

<template>
  <ul v-if="rows.length > 0" class="stream-pl-list" :class="{ 'stream-pl-list--fill': fill }">
    <li v-for="row in rows" :key="row.key" class="stream-pl-item">
      <button type="button" class="stream-pl-play" @click="emit('play', row.job)">
        <span class="stream-pl-title-row">
          <span v-if="openedForJob?.(row.job)" class="stream-pl-opened" title="已開啟過">●</span>
          <span class="stream-pl-title">{{ row.job.title }}</span>
        </span>
        <span class="stream-pl-meta">{{ row.job.relPath }}</span>
        <span v-if="progressForJob?.(row.job)" class="stream-pl-progress">{{ progressForJob(row.job) }}</span>
      </button>
      <button
        v-if="showRemove"
        type="button"
        class="stream-pl-remove"
        title="移除"
        @click="emit('remove', row.job)"
      >
        ×
      </button>
    </li>
  </ul>
  <p v-else class="stream-pl-empty">{{ emptyText ?? '（空）' }}</p>
</template>

<style scoped>
.stream-pl-list {
  list-style: none;
  margin: 0;
  padding: 0;
  max-height: clamp(120px, 26dvh, 360px);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.stream-pl-list--fill {
  flex: 1;
  min-height: 0;
  max-height: none;
}
.stream-pl-item {
  display: flex;
  align-items: stretch;
  gap: 4px;
}
.stream-pl-play {
  flex: 1;
  min-width: 0;
  text-align: left;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid rgba(255, 255, 255, 0.12);
  background: rgba(0, 0, 0, 0.2);
  color: inherit;
  cursor: pointer;
}
.stream-pl-play:disabled {
  opacity: 0.5;
}
.stream-pl-title-row {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}
.stream-pl-opened {
  flex-shrink: 0;
  color: #6eb5ff;
  font-size: 10px;
  line-height: 1;
}
.stream-pl-title {
  display: block;
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  min-width: 0;
}
.stream-pl-progress {
  display: block;
  font-size: 11px;
  color: #9ab;
  margin-top: 2px;
  line-height: 1.3;
}
.stream-pl-meta {
  display: block;
  font-size: 10px;
  opacity: 0.65;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 2px;
}
.stream-pl-remove {
  flex-shrink: 0;
  width: 32px;
  border: none;
  border-radius: 8px;
  background: rgba(255, 80, 80, 0.2);
  color: #ffb4b4;
  font-size: 18px;
  line-height: 1;
}
.stream-pl-empty {
  margin: 8px 0 0;
  font-size: 12px;
  opacity: 0.7;
}
</style>
