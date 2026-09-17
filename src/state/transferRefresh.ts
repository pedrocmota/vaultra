import type {TransferItem} from '@/lib/api'
import {normalizeLocal, parentLocal, parentRemote} from '@/lib/format'
import type {PaneSide, Tab} from '@/state/store'

const REFRESH_DELAY_MS = 1000

type RefreshFn = (tabId: string, side: PaneSide) => Promise<void>

interface PaneRef {
  tabId: string,
  side: PaneSide
}

function normalizeRemote(path: string): string {
  return path.replace(/\/+$/, '') || '/'
}

function isSameOrAncestor(pane: string, destination: string, separator: string): boolean {
  if (pane === destination) {
    return true
  }

  const prefix = pane.endsWith(separator) ? pane : pane + separator

  return destination.startsWith(prefix)
}

function affectedPanes(item: TransferItem, tabs: Tab[]): PaneRef[] {
  if (item.direction === 'download') {
    const destination = normalizeLocal(parentLocal(item.localPath))

    return tabs
      .filter((tab) => isSameOrAncestor(normalizeLocal(tab.local.path), destination, '\\'))
      .map((tab) => ({tabId: tab.id, side: 'local' as const}))
  }

  const remoteTarget = item.direction === 'copy' ? (item.targetPath ?? '') : item.remotePath
  const targetSession = item.direction === 'copy' ? (item.targetSessionId ?? item.sessionId) : item.sessionId
  const destination = normalizeRemote(parentRemote(remoteTarget))

  return tabs
    .filter((tab) => tab.sessionId !== null && tab.sessionId === targetSession)
    .filter((tab) => isSameOrAncestor(normalizeRemote(tab.remote.path), destination, '/'))
    .map((tab) => ({tabId: tab.id, side: 'remote' as const}))
}

export function createTransferRefresher(refresh: RefreshFn, tabs: () => Tab[]) {
  const pending = new Map<string, PaneRef>()
  let timer: number | null = null

  const flush = () => {
    timer = null
    const targets = [...pending.values()]
    pending.clear()

    for (const target of targets) {
      void refresh(target.tabId, target.side)
    }
  }

  const schedule = (pane: PaneRef) => {
    pending.set(`${pane.tabId}:${pane.side}`, pane)

    if (timer === null) {
      timer = window.setTimeout(flush, REFRESH_DELAY_MS)
    }
  }

  return (previous: TransferItem[], next: TransferItem[]) => {
    const known = new Set(previous.map((item) => item.id))
    const completed = next.filter((item) => item.status === 'done' && !known.has(item.id))

    for (const item of completed) {
      affectedPanes(item, tabs()).forEach(schedule)
    }
  }
}