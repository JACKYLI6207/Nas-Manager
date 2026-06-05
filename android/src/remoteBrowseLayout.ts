/** 遠端管理瀏覽顯示方式（與站內搜索 layout 分開儲存） */
export type RemoteBrowseLayout = 'list' | 'grid2' | 'grid4' | 'grid6' | 'grid8' | 'grid10'

const LAYOUT_KEY = 'nas-remote-browse-layout'

export const REMOTE_BROWSE_LAYOUT_OPTIONS: { key: RemoteBrowseLayout; label: string }[] = [
  { key: 'grid2', label: '每排 2 個' },
  { key: 'grid4', label: '每排 4 個' },
  { key: 'grid6', label: '每排 6 個' },
  { key: 'grid8', label: '每排 8 個' },
  { key: 'grid10', label: '每排 10 個' },
  { key: 'list', label: '列表顯示' },
]

const VALID = new Set<RemoteBrowseLayout>(['list', 'grid2', 'grid4', 'grid6', 'grid8', 'grid10'])

export function gridColsForRemoteBrowseLayout(layout: RemoteBrowseLayout): number {
  switch (layout) {
    case 'grid2':
      return 2
    case 'grid6':
      return 6
    case 'grid8':
      return 8
    case 'grid10':
      return 10
    case 'grid4':
      return 4
    case 'list':
    default:
      return 1
  }
}

export function isRemoteBrowseGridLayout(layout: RemoteBrowseLayout): boolean {
  return layout !== 'list'
}

export function loadSavedRemoteBrowseLayout(): RemoteBrowseLayout {
  try {
    const saved = localStorage.getItem(LAYOUT_KEY)
    if (saved && VALID.has(saved as RemoteBrowseLayout)) {
      return saved as RemoteBrowseLayout
    }
  } catch {
    /* ignore */
  }
  return 'list'
}

export function saveRemoteBrowseLayout(layout: RemoteBrowseLayout) {
  try {
    localStorage.setItem(LAYOUT_KEY, layout)
  } catch {
    /* ignore */
  }
}

export function remoteBrowseLayoutLabel(layout: RemoteBrowseLayout): string {
  return REMOTE_BROWSE_LAYOUT_OPTIONS.find((o) => o.key === layout)?.label ?? '列表顯示'
}
