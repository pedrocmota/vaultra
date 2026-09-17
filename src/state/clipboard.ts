import {api} from '@/lib/api'
import {isDirLike, type Entry} from '@/lib/entries'
import {buildUrl, joinRemote, uniqueName} from '@/lib/format'
import {
  buildDownloadRequests,
  buildRemoteCopyRequests,
  buildUploadRequests,
  copyLocalPaths,
  enqueueTransfers,
  toastError,
  uploadLocalPaths
} from './actions'
import {t, tabById, useStore, type PaneSide, type Tab} from './store'

interface ClipboardContent {
  token: string,
  side: PaneSide,
  sessionId: string | null,
  sourceDir: string,
  entries: Entry[]
}

let content: ClipboardContent | null = null

export async function copyEntries(tabId: string, side: PaneSide, entries: Entry[]) {
  const tab = tabById(tabId)

  if (!tab || entries.length === 0) {
    return
  }

  const paths = entries.map((entry) => entry.path)
  const text =
    side === 'remote' && tab.site
      ? entries.map((entry) => buildUrl(tab.site!, entry.path)).join('\n')
      : paths.join('\n')

  const token = crypto.randomUUID()

  try {
    await api.clipboardWrite(text, side === 'local' ? paths : null, token)
    content = {
      token,
      side,
      sessionId: side === 'remote' ? tab.sessionId : null,
      sourceDir: tab[side].path,
      entries
    }
    useStore.getState().pushToast(t('clipboard.copied', {n: entries.length}), 'info')
  } catch (error) {
    toastError(error)
  }
}

export async function pasteEntries(tabId: string, side: PaneSide) {
  const tab = tabById(tabId)

  if (!tab || (side === 'remote' && !tab.sessionId)) {
    return
  }

  let external

  try {
    external = await api.clipboardReadFiles()
  } catch (error) {
    toastError(error)

    return
  }

  if (content && external.token === content.token) {
    await pasteInternal(tab, side, content)

    return
  }

  content = null

  if (external.paths.length === 0) {
    useStore.getState().pushToast(t('clipboard.nothing'), 'info')

    return
  }

  if (side === 'remote') {
    await uploadLocalPaths(tabId, external.paths)
  } else {
    await copyLocalPaths(tabId, external.paths)
  }
}

async function pasteInternal(tab: Tab, side: PaneSide, source: ClipboardContent) {
  const destDir = tab[side].path

  if (source.side === 'local') {
    if (side === 'local') {
      await copyLocalPaths(tab.id, source.entries.map((entry) => entry.path))
    } else {
      await enqueueTransfers(buildUploadRequests(tab.sessionId!, source.entries, destDir))
    }

    return
  }

  if (!source.sessionId) {
    return
  }

  if (side === 'local') {
    await enqueueTransfers(buildDownloadRequests(source.sessionId, source.entries, destDir))

    return
  }

  const targets = remoteCopyTargets(source, tab, destDir)

  if (!targets) {
    useStore.getState().pushToast(t('clipboard.intoItself'), 'error')

    return
  }

  await enqueueTransfers(
    buildRemoteCopyRequests(source.sessionId, source.entries, tab.sessionId!, targets)
  )
}

function remoteCopyTargets(source: ClipboardContent, tab: Tab, destDir: string): string[] | null {
  const sameDir = source.sessionId === tab.sessionId && source.sourceDir === destDir
  const taken = new Set(tab.remote.entries.map((entry) => entry.name))
  const targets: string[] = []

  for (const entry of source.entries) {
    let name = entry.name

    if (sameDir) {
      name = uniqueName(entry.name, (candidate) => taken.has(candidate))
      taken.add(name)
    }

    const target = joinRemote(destDir, name)
    const sameServer = source.sessionId === tab.sessionId
    const insideItself = target === entry.path || target.startsWith(`${entry.path}/`)

    if (isDirLike(entry) && sameServer && insideItself) {
      return null
    }

    targets.push(target)
  }

  return targets
}