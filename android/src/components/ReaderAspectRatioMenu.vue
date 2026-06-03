<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import {
  ASPECT_RATIO_OPTIONS,
  readerAspectRatio,
  setReaderAspectRatio,
  type ReaderAspectRatio,
} from '../composables/useReaderAspectRatio'

const menuOpen = ref(false)
const rootRef = ref<HTMLElement | null>(null)

function toggleMenu() {
  menuOpen.value = !menuOpen.value
}

function select(mode: ReaderAspectRatio) {
  setReaderAspectRatio(mode)
  menuOpen.value = false
}

function onDocumentPointerDown(event: Event) {
  if (!menuOpen.value || !rootRef.value) return
  const target = event.target
  if (target instanceof Node && !rootRef.value.contains(target)) {
    menuOpen.value = false
  }
}

onMounted(() => {
  document.addEventListener('pointerdown', onDocumentPointerDown, true)
})

onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocumentPointerDown, true)
})
</script>

<template>
  <div ref="rootRef" class="reader-aspect-menu" @click.stop>
    <button
      type="button"
      class="reader-btn reader-aspect-trigger"
      :class="{ 'reader-aspect-trigger--open': menuOpen }"
      @click.stop="toggleMenu"
    >
      顯示比例{{ menuOpen ? ' ▴' : ' ▾' }}
    </button>
    <div v-if="menuOpen" class="reader-aspect-dropdown" @click.stop>
      <button
        v-for="opt in ASPECT_RATIO_OPTIONS"
        :key="opt.id"
        type="button"
        class="reader-aspect-option"
        :class="{ 'reader-aspect-option--on': readerAspectRatio === opt.id }"
        @click="select(opt.id)"
      >
        <span>{{ opt.label }}</span>
        <span v-if="readerAspectRatio === opt.id" class="reader-aspect-check" aria-hidden="true">✓</span>
      </button>
    </div>
  </div>
</template>
