import type {PaneSide} from '@/state/store'

export interface DropTarget {
  tabId: string,
  side: PaneSide,
  destDir?: string
}

export interface DropHighlighter {
  update: (target: Element | null, accept: (pane: HTMLElement) => boolean) => void,
  clear: () => void
}

function paneOf(target: Element | null): HTMLElement | null {
  return target?.closest<HTMLElement>('.pane[data-side]') ?? null
}

function folderRowOf(target: Element | null): HTMLElement | null {
  const row = target?.closest<HTMLElement>('.row[data-path]') ?? null

  return row && row.dataset.dir === '1' ? row : null
}

export function resolveDropTarget(target: Element | null): DropTarget | null {
  const pane = paneOf(target)

  if (!pane || !pane.dataset.tab) {
    return null
  }

  return {
    tabId: pane.dataset.tab,
    side: pane.dataset.side as PaneSide,
    destDir: folderRowOf(target)?.dataset.path
  }
}

export function createDropHighlighter(): DropHighlighter {
  let list: Element | null = null
  let row: Element | null = null

  const clear = () => {
    list?.classList.remove('drop-target')
    row?.classList.remove('drop-into')
    list = null
    row = null
  }

  const update = (target: Element | null, accept: (pane: HTMLElement) => boolean) => {
    const pane = paneOf(target)
    const accepted = pane && accept(pane) ? pane.querySelector('.filelist') : null
    const nextRow = accepted ? folderRowOf(target) : null
    const nextList = nextRow ? null : accepted

    if (nextList !== list) {
      list?.classList.remove('drop-target')
      nextList?.classList.add('drop-target')
      list = nextList
    }

    if (nextRow !== row) {
      row?.classList.remove('drop-into')
      nextRow?.classList.add('drop-into')
      row = nextRow
    }
  }

  return {update, clear}
}