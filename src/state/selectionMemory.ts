import type {Entry} from '@/lib/entries'
import {normalizeLocal, parentLocal, parentRemote} from '@/lib/format'
import type {PaneSelection, PaneSide} from '@/state/store'

function key(side: PaneSide, path: string): string {
  return side === 'local' ? normalizeLocal(path) : path.replace(/\/+$/, '') || '/'
}

function samePath(side: PaneSide, a: string, b: string): boolean {
  return key(side, a) === key(side, b)
}

function parentOf(side: PaneSide, path: string): string {
  return side === 'local' ? parentLocal(path) : parentRemote(path)
}

export function selectionAfterNavigation(
  side: PaneSide,
  previousPath: string,
  path: string,
  entries: Entry[]
): PaneSelection {
  if (previousPath && samePath(side, parentOf(side, previousPath), path)) {
    const origin = entries.find((e) => samePath(side, e.path, previousPath))

    if (origin) {
      return {cursor: origin.path, selected: [origin.path]}
    }
  }

  return {cursor: null, selected: []}
}