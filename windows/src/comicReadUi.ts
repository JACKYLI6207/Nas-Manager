import { ref } from 'vue'

export type ComicReadSubTab = 'home' | 'streamList' | 'streamFav' | 'streamLog'

export const comicReadSubTab = ref<ComicReadSubTab>('home')

/** 從遠端管理加入串流閱讀時，切換到漫畫閱讀分頁 */
export const comicStreamNavigateSeq = ref(0)

export function bumpComicStreamNavigate() {
  comicStreamNavigateSeq.value += 1
}
