<script setup lang="ts">
import { API_DOMAIN_OPTIONS } from '../apiDomains'

const props = defineProps<{
  showing: boolean
  selectedDomain: string
}>()

const emit = defineEmits<{
  'update:showing': [showing: boolean]
  select: [domain: string]
}>()

function closeDialog() {
  emit('update:showing', false)
}

function pick(domain: string) {
  emit('select', domain)
  emit('update:showing', false)
}
</script>

<template>
  <div
    v-if="showing"
    class="api-domain-overlay"
    @click.self="closeDialog"
    @touchstart.self="closeDialog"
  >
    <div class="api-domain-dialog" @click.stop @touchstart.stop>
      <div class="api-domain-header">
        <p class="api-domain-title">API域名列表</p>
        <button type="button" class="api-domain-close" aria-label="關閉" @click.stop="closeDialog">
          ×
        </button>
      </div>
      <ul class="api-domain-list">
        <li v-for="domain in API_DOMAIN_OPTIONS" :key="domain">
          <button
            type="button"
            class="api-domain-item"
            :class="{ 'api-domain-item--on': domain === selectedDomain }"
            @click="pick(domain)"
          >
            <span>{{ domain }}</span>
            <span v-if="domain === selectedDomain" class="api-domain-check" aria-hidden="true">✓</span>
          </button>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.api-domain-overlay {
  position: fixed;
  inset: 0;
  z-index: 1200;
  display: flex;
  align-items: flex-end;
  justify-content: center;
  background: rgba(0, 0, 0, 0.55);
  padding: 12px;
}

.api-domain-dialog {
  width: 100%;
  max-width: 420px;
  max-height: min(72vh, 520px);
  display: flex;
  flex-direction: column;
  background: #1e1e1e;
  border-radius: 12px 12px 0 0;
  border: 1px solid #3c4043;
  overflow: hidden;
}

.api-domain-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 14px 8px;
  border-bottom: 1px solid #3c4043;
  flex-shrink: 0;
}

.api-domain-title {
  margin: 0;
  font-size: 14px;
  font-weight: 700;
  color: #e8eaed;
}

.api-domain-close {
  border: none;
  background: transparent;
  color: #9aa0a6;
  font-size: 22px;
  line-height: 1;
  padding: 0 4px;
}

.api-domain-list {
  list-style: none;
  margin: 0;
  padding: 6px 0 12px;
  overflow-y: auto;
  -webkit-overflow-scrolling: touch;
}

.api-domain-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  border: none;
  background: transparent;
  color: #e8eaed;
  font-size: 13px;
  text-align: left;
  padding: 10px 16px;
}

.api-domain-item--on {
  background: rgba(61, 110, 245, 0.22);
  color: #8ab4f8;
}

.api-domain-check {
  font-size: 14px;
  color: #8ab4f8;
}
</style>
