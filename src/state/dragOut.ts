import {api, type DragOutRequest} from '@/lib/api'
import type {Entry} from '@/lib/entries'
import {buildDownloadRequests, refresh, toastError} from './actions'
import {tabById, useStore, type PaneSide} from './store'

let active = false

export function isNativeDragActive(): boolean {
  return active
}

function buildRequest(tabId: string, side: PaneSide, entries: Entry[]): DragOutRequest | null {
  const tab = tabById(tabId)

  if (!tab) {
    return null
  }

  if (side === 'local') {
    return {localPaths: entries.map((entry) => entry.path)}
  }

  if (!tab.sessionId) {
    return null
  }

  return {downloads: buildDownloadRequests(tab.sessionId, entries, '')}
}

export async function dragOutEntries(tabId: string, side: PaneSide, entries: Entry[]) {
  const request = buildRequest(tabId, side, entries)

  if (!request || entries.length === 0 || active) {
    return
  }

  active = true

  if (side === 'remote') {
    useStore.getState().setBottomTab('queue')
  }

  try {
    const result = await api.dragOut(request)

    if (side === 'local' && result.outcome === 'dropped') {
      await refresh(tabId, 'local')
    }
  } catch (error) {
    toastError(error)
  } finally {
    active = false
  }
}