import {listen} from '@tauri-apps/api/event'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {open as openFileDialog} from '@tauri-apps/plugin-dialog'
import {
  api,
  describeError,
  isHostKeyChanged,
  isUntrustedCertificate,
  type AppSettings,
  type ConflictAction,
  type ConflictPrompt,
  type EditedFileChanged,
  type LogMessage,
  type ProgressEvent,
  type QueueRequest,
  type QueueSnapshot,
  type QueueStats,
  type SiteConfig,
  type Theme,
  type UiPrompt
} from '@/lib/api'
import {fromLocal, fromRemote, isDirLike, isExecutable, isLink, type Entry} from '@/lib/entries'
import {
  basename,
  buildUrl,
  joinLocal,
  joinRemote,
  normalizeLocal,
  parentLocal,
  parentRemote,
  relativeLocal,
  relativeRemote,
  uniqueName
} from '@/lib/format'
import {selectionAfterNavigation} from './selectionMemory'
import {activeTab, createTab, t, tabById, useStore, type PaneSide, type Tab} from './store'
import {createTransferRefresher} from './transferRefresh'

const store = useStore

let systemThemeQuery: MediaQueryList | null = null

export function applyTheme(theme: Theme) {
  const root = document.documentElement
  const resolve = () => {
    if (theme === 'system') {
      return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
    }

    return theme
  }
  root.setAttribute('data-theme', resolve())

  if (systemThemeQuery) {
    systemThemeQuery.onchange = null
    systemThemeQuery = null
  }

  if (theme === 'system') {
    systemThemeQuery = window.matchMedia('(prefers-color-scheme: light)')
    systemThemeQuery.onchange = () => root.setAttribute('data-theme', resolve())
  }
}

export async function revealWindow() {
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  const window = getCurrentWindow()
  await window.show().catch(() => undefined)
  await window.setFocus().catch(() => undefined)
}

export async function bootstrap() {
  const [settings, sites, system, queue] = await Promise.all([
    api.settingsGet(),
    api.sitesGet(),
    api.systemInfo(),
    api.queueSnapshot()
  ])
  store.getState().setSettings(settings)
  store.getState().setSites(sites)
  store.getState().setSystem(system)
  store.getState().setQueue(queue)
  applyTheme(settings.theme)
  document.documentElement.lang = settings.language
  await subscribeEvents()

  if (store.getState().tabs.length === 0) {
    await newTab()
  }
}

async function subscribeEvents() {
  await listen<LogMessage>('log', (event) => store.getState().appendLog(event.payload))
  await listen<UiPrompt>('auth-prompt', (event) =>
    store.getState().openDialog({kind: 'prompt', prompt: event.payload})
  )
  await listen<ConflictPrompt>('conflict-prompt', (event) =>
    store.getState().openDialog({kind: 'conflict', prompt: event.payload})
  )
  await listen<QueueSnapshot>('queue-changed', (event) => {
    refreshCompletedTransfers(store.getState().queue.history, event.payload.history)
    store.getState().setQueue(event.payload)
  })
  await listen<QueueStats>('queue-stats', (event) => store.getState().setStats(event.payload))
  await listen<ProgressEvent>('transfer-progress', (event) =>
    store.getState().applyProgress(event.payload)
  )
  await listen<EditedFileChanged>('edited-file-changed', (event) =>
    store.getState().openDialog({kind: 'editChanged', event: event.payload})
  )
}

const refreshCompletedTransfers = createTransferRefresher(refresh, () => store.getState().tabs)

export function toastError(error: unknown) {
  store.getState().pushToast(describeError(error), 'error')
}

export async function newTab(): Promise<Tab> {
  const settings = store.getState().settings
  const current = activeTab()
  const localPath = current?.local.path || settings.defaultLocalDir || (await api.localHome())
  const tab = createTab(localPath)
  store.getState().addTab(tab)
  await navigate(tab.id, 'local', localPath, {pushHistory: false})

  return tab
}

export async function closeTab(tabId: string) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  if (tab.sessionId) {
    await api.sessionDisconnect(tab.sessionId).catch(() => undefined)
  }

  store.getState().removeTab(tabId)

  if (store.getState().tabs.length === 0) {
    await newTab()
  }
}

export async function disconnectTab(tabId: string) {
  const tab = tabById(tabId)

  if (!tab?.sessionId) {
    return
  }

  await api.sessionDisconnect(tab.sessionId).catch(() => undefined)
  store
    .getState()
    .updateTab(tabId, {sessionId: null, site: null, syncBrowsing: false, syncBase: null})
  store
    .getState()
    .updatePane(tabId, 'remote', {
      path: '',
      entries: [],
      selected: [],
      cursor: null,
      error: null,
      history: [],
      historyIndex: -1
    })
}

interface ConnectOptions {
  tabId?: string,
  password?: string | null,
  acceptNewHostkey?: boolean,
  remember?: boolean,
  bookmark?: {localDir: string, remoteDir: string, syncBrowsing: boolean} | null
}

export async function connectSite(site: SiteConfig, options: ConnectOptions = {}) {
  let tabId = options.tabId
  let tab = tabId ? tabById(tabId) : activeTab()

  if (!tab || tab.sessionId || tab.connecting) {
    tab = await newTab()
  }

  tabId = tab.id

  if (site.logonType === 'ask' && options.password === undefined) {
    store.getState().openDialog({kind: 'password', site, tabId})

    return
  }

  const effectiveSite = options.bookmark
    ? {
      ...site,
      localDir: options.bookmark.localDir || site.localDir,
      remoteDir: options.bookmark.remoteDir || site.remoteDir,
      syncBrowsing: options.bookmark.syncBrowsing
    }
    : site
  store.getState().updateTab(tabId, {connecting: true, site: effectiveSite})
  store.getState().updatePane(tabId, 'remote', {error: null, entries: [], path: ''})

  try {
    const password =
      site.logonType === 'anonymous' || site.logonType === 'key_file'
        ? null
        : (options.password ?? null)

    const info = await api.sessionConnect(
      effectiveSite,
      password,
      options.acceptNewHostkey ?? false,
      options.remember ?? true
    )
    store.getState().updateTab(tabId, {
      sessionId: info.id,
      site: info.site,
      connecting: false,
      syncBrowsing: effectiveSite.syncBrowsing,
      syncBase: null
    })
    store.getState().updatePane(tabId, 'remote', {history: [], historyIndex: -1})

    if (effectiveSite.localDir) {
      await navigate(tabId, 'local', effectiveSite.localDir, {pushHistory: false})
    }

    await navigate(tabId, 'remote', info.initialDir, {pushHistory: true})

    if (effectiveSite.syncBrowsing) {
      const current = tabById(tabId)

      if (current) {
        store
          .getState()
          .updateTab(tabId, {
            syncBase: {local: current.local.path, remote: current.remote.path}
          })
      }
    }

    store.getState().setFocusedPane('remote')
  } catch (error) {
    store.getState().updateTab(tabId, {connecting: false, sessionId: null})

    if (isHostKeyChanged(error)) {
      store
        .getState()
        .openDialog({
          kind: 'hostKeyChanged',
          detail: error.detail,
          site,
          tabId,
          password: options.password ?? null
        })

      return
    }

    if (isUntrustedCertificate(error)) {
      store
        .getState()
        .openDialog({
          kind: 'certificate',
          detail: error.detail,
          site,
          tabId,
          password: options.password ?? null
        })

      return
    }

    store.getState().updatePane(tabId, 'remote', {error: describeError(error)})
    toastError(error)
  }
}

export async function acceptChangedHostKey(
  tabId: string,
  site: SiteConfig,
  detail: {host: string},
  password: string | null
) {
  const [host, portText] = splitHostPort(detail.host, site)

  try {
    await api.hostkeyForget(host, portText)
  } catch (error) {
    toastError(error)

    return
  }

  await connectSite(site, {tabId, password, acceptNewHostkey: true})
}

function splitHostPort(value: string, site: SiteConfig): [string, number] {
  const match = value.match(/^\[?([^\]]+?)\]?:(\d+)$/)

  if (match) {
    return [match[1], Number(match[2])]
  }

  return [site.host, site.port]
}

export async function acceptCertificate(
  tabId: string,
  site: SiteConfig,
  fingerprint: string,
  remember: boolean,
  password: string | null
) {
  await api.certificateTrust(fingerprint, remember)
  await connectSite(site, {tabId, password})
}

interface NavigateOptions {
  pushHistory?: boolean,
  force?: boolean,
  keepSelection?: boolean,
  mirror?: boolean
}

export async function navigate(
  tabId: string,
  side: PaneSide,
  path: string,
  options: NavigateOptions = {}
) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  const {pushHistory = true, force = false, keepSelection = false, mirror = true} = options
  store.getState().updatePane(tabId, side, {loading: true, error: null})

  try {
    let entries: Entry[]
    let resolved = path

    if (side === 'remote') {
      if (!tab.sessionId) {
        store.getState().updatePane(tabId, side, {loading: false})

        return
      }

      const listing = await api.remoteList(tab.sessionId, path, force)
      resolved = listing.path
      entries = listing.entries.map(fromRemote)
    } else {
      const listing = await api.localList(path, store.getState().settings.showHidden)
      resolved = listing.path
      entries = listing.entries.map(fromLocal)
    }

    store.getState().updatePane(tabId, side, (pane) => {
      const changed = pane.path !== resolved
      let history = pane.history
      let historyIndex = pane.historyIndex

      if (pushHistory && changed) {
        history = [...history.slice(0, historyIndex + 1), resolved].slice(-100)
        historyIndex = history.length - 1
      } else if (!pushHistory && history.length === 0) {
        history = [resolved]
        historyIndex = 0
      }

      const selection = keepSelection
        ? {
          cursor: pane.cursor,
          selected: pane.selected.filter((p) => entries.some((e) => e.path === p))
        }
        : selectionAfterNavigation(side, pane.path, resolved, entries)

      return {
        path: resolved,
        entries,
        loading: false,
        error: null,
        ...selection,
        history,
        historyIndex,
        filter: changed ? '' : pane.filter
      }
    })

    if (mirror) {
      await mirrorNavigation(tabId, side, resolved)
    }
  } catch (error) {
    store.getState().updatePane(tabId, side, {loading: false, error: describeError(error)})
  }
}

async function mirrorNavigation(tabId: string, side: PaneSide, resolved: string) {
  const tab = tabById(tabId)

  if (!tab || !tab.syncBrowsing || !tab.syncBase || !tab.sessionId) {
    return
  }

  const other: PaneSide = side === 'local' ? 'remote' : 'local'
  const relative =
    side === 'local'
      ? relativeLocal(tab.syncBase.local, resolved)
      : relativeRemote(tab.syncBase.remote, resolved)

  if (relative === null) {
    return
  }

  const target =
    other === 'local'
      ? relative
        ? joinLocal(tab.syncBase.local, relative.replace(/\//g, '\\'))
        : tab.syncBase.local
      : relative
        ? joinRemote(tab.syncBase.remote, relative)
        : tab.syncBase.remote

  if (target === tab[other].path) {
    return
  }

  await navigate(tabId, other, target, {mirror: false})
}

export async function refresh(tabId: string, side: PaneSide) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  const path = tab[side].path

  if (side === 'remote' && !tab.sessionId) {
    return
  }

  await navigate(tabId, side, path, {
    pushHistory: false,
    force: true,
    keepSelection: true,
    mirror: false
  })
}

export async function goUp(tabId: string, side: PaneSide) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  const path = tab[side].path

  if (side === 'remote') {
    if (!path || path === '/') {
      return
    }

    await navigate(tabId, side, parentRemote(path))
  } else {
    if (!path) {
      return
    }

    const trimmed = path.replace(/[\\/]+$/, '')
    const index = trimmed.lastIndexOf('\\')

    if (index < 0 || /^[A-Za-z]:$/.test(trimmed)) {
      await navigate(tabId, side, '')
    } else {
      const parent = trimmed.slice(0, index)
      await navigate(tabId, side, /^[A-Za-z]:$/.test(parent) ? `${parent}\\` : parent)
    }
  }
}

export async function goHistory(tabId: string, side: PaneSide, delta: number) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  const pane = tab[side]
  const index = pane.historyIndex + delta

  if (index < 0 || index >= pane.history.length) {
    return
  }

  store.getState().updatePane(tabId, side, {historyIndex: index})
  await navigate(tabId, side, pane.history[index], {pushHistory: false})
}

export async function openEntry(tabId: string, side: PaneSide, entry: Entry) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  if (entry.kind === 'dir' || entry.kind === 'drive') {
    await navigate(tabId, side, entry.path)

    return
  }

  if (isLink(entry)) {
    const target = await resolveLink(tab, side, entry)

    if (!target) {
      return
    }

    if (target.isDir) {
      await navigate(tabId, side, target.target)
    } else {
      await openFile(tabId, side, {...entry, path: target.target, kind: 'file'})
    }

    return
  }

  await openFile(tabId, side, entry)
}

async function openFile(tabId: string, side: PaneSide, entry: Entry) {
  if (isExecutable(entry)) {
    return
  }

  await viewEdit(tabId, side, entry)
}

async function resolveLink(tab: Tab, side: PaneSide, entry: Entry) {
  try {
    if (side === 'remote') {
      if (!tab.sessionId) {
        return null
      }

      return await api.remoteResolveLink(tab.sessionId, entry.path)
    }

    return await api.localResolveLink(entry.path)
  } catch (error) {
    toastError(error)

    return null
  }
}

export async function goToTarget(tabId: string, side: PaneSide, entry: Entry) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  const target = await resolveLink(tab, side, entry)

  if (!target) {
    return
  }

  if (target.isDir) {
    await navigate(tabId, side, target.target)
  } else {
    const parent =
      side === 'remote'
        ? parentRemote(target.target)
        : target.target.slice(0, Math.max(target.target.lastIndexOf('\\'), 0)) || target.target
    await navigate(tabId, side, parent)
    store.getState().updatePane(tabId, side, {selected: [target.target], cursor: target.target})
  }
}

export interface TransferOptions {
  priority?: number,
  destDir?: string,
  startPaused?: boolean
}

export interface RequestOptions {
  priority?: number,
  startPaused?: boolean
}

function followSymlinks(): boolean {
  return store.getState().settings.symlinkDownload === 'follow'
}

export function buildDownloadRequests(
  sessionId: string,
  entries: Entry[],
  localDest: string,
  options: RequestOptions = {}
): QueueRequest[] {
  const followSymlink = followSymlinks()

  return entries.map((entry) => {
    const dirLike = isDirLike(entry)
    const copyLink = !followSymlink && isLink(entry)

    return {
      sessionId,
      direction: 'download',
      localPath: joinLocal(localDest, entry.name),
      remotePath: entry.path,
      isDir: dirLike && !copyLink,
      size: dirLike || copyLink ? null : entry.size,
      mtime: entry.mtime,
      priority: options.priority ?? 0,
      followSymlink,
      startPaused: options.startPaused ?? false,
      linkTarget: copyLink ? (entry.linkTarget ?? '') : null
    }
  })
}

export function buildUploadRequests(
  sessionId: string,
  entries: Entry[],
  remoteDest: string,
  options: RequestOptions = {}
): QueueRequest[] {
  const followSymlink = followSymlinks()

  return entries.map((entry) => {
    const dirLike = isDirLike(entry)

    return {
      sessionId,
      direction: 'upload',
      localPath: entry.path,
      remotePath: joinRemote(remoteDest, entry.name),
      isDir: dirLike,
      size: dirLike ? null : entry.size,
      mtime: entry.mtime,
      priority: options.priority ?? 0,
      followSymlink,
      startPaused: options.startPaused ?? false
    }
  })
}

export function buildRemoteCopyRequests(
  sessionId: string,
  entries: Entry[],
  targetSessionId: string,
  targets: string[]
): QueueRequest[] {
  const followSymlink = followSymlinks()

  return entries.map((entry, index) => {
    const dirLike = isDirLike(entry)

    return {
      sessionId,
      direction: 'copy',
      localPath: '',
      remotePath: entry.path,
      isDir: dirLike,
      size: dirLike ? null : entry.size,
      mtime: entry.mtime,
      followSymlink,
      targetSessionId,
      targetPath: targets[index]
    }
  })
}

export async function enqueueTransfers(requests: QueueRequest[]) {
  if (requests.length === 0) {
    return
  }

  try {
    await api.queueAdd(requests)
    store.getState().setBottomTab('queue')
  } catch (error) {
    toastError(error)
  }
}

export async function transferEntries(
  tabId: string,
  fromSide: PaneSide,
  entries: Entry[],
  options: TransferOptions = {}
) {
  const tab = tabById(tabId)

  if (!tab || !tab.sessionId) {
    return
  }

  const requestOptions = {priority: options.priority, startPaused: options.startPaused}
  const requests =
    fromSide === 'remote'
      ? buildDownloadRequests(
        tab.sessionId,
        entries,
        options.destDir ?? tab.local.path,
        requestOptions
      )
      : buildUploadRequests(
        tab.sessionId,
        entries,
        options.destDir ?? tab.remote.path,
        requestOptions
      )
  await enqueueTransfers(requests)
}

export async function copyLocalPaths(tabId: string, sources: string[], destDir?: string) {
  const tab = tabById(tabId)
  const dir = destDir ?? tab?.local.path

  if (!tab || sources.length === 0) {
    return
  }

  if (!dir) {
    store.getState().pushToast(t('clipboard.noTarget'), 'info')

    return
  }

  let existing: Set<string>

  try {
    const listing = await api.localList(dir, true)
    existing = new Set(listing.entries.map((entry) => entry.name.toLowerCase()))
  } catch (error) {
    toastError(error)

    return
  }

  const taken = new Set(existing)
  const items: {source: string, destination: string}[] = []
  let conflicts = 0

  for (const source of sources) {
    const name = basename(source)
    const sameDir = normalizeLocal(parentLocal(source)) === normalizeLocal(dir)

    if (sameDir) {
      const fresh = uniqueName(name, (candidate) => taken.has(candidate.toLowerCase()))
      taken.add(fresh.toLowerCase())
      items.push({source, destination: joinLocal(dir, fresh)})
    } else {
      if (existing.has(name.toLowerCase())) {
        conflicts += 1
      }

      items.push({source, destination: joinLocal(dir, name)})
    }
  }

  const run = async (overwrite: boolean) => {
    store.getState().updatePane(tabId, 'local', {loading: true})

    try {
      await api.localCopy(items, overwrite)
    } catch (error) {
      toastError(error)
    }

    await refresh(tabId, 'local')
  }

  if (conflicts === 0) {
    await run(false)

    return
  }

  store.getState().openDialog({
    kind: 'confirm',
    title: t('dialog.replaceTitle'),
    message: t('dialog.replaceConfirm', {n: conflicts}),
    confirmLabel: t('dialog.replace'),
    onConfirm: () => run(true)
  })
}

export async function uploadLocalPaths(tabId: string, paths: string[], remoteDir?: string) {
  const tab = tabById(tabId)

  if (!tab?.sessionId) {
    return
  }

  const settings = store.getState().settings
  const requests: QueueRequest[] = []
  for (const path of paths) {
    const listing = await api
      .localList(path.slice(0, Math.max(path.lastIndexOf('\\'), 0)) || path, true)
      .catch(() => null)

    const entry = listing?.entries.find((e) => e.path.toLowerCase() === path.toLowerCase())
    const converted = entry ? fromLocal(entry) : null
    const dirLike = converted ? isDirLike(converted) : false
    requests.push({
      sessionId: tab.sessionId,
      direction: 'upload',
      localPath: path,
      remotePath: joinRemote(remoteDir ?? tab.remote.path, basename(path)),
      isDir: dirLike,
      size: dirLike ? null : (converted?.size ?? null),
      mtime: converted?.mtime ?? null,
      followSymlink: settings.symlinkDownload === 'follow'
    })
  }

  if (requests.length) {
    await api.queueAdd(requests).catch(toastError)
    store.getState().setBottomTab('queue')
  }
}

export function selectedEntries(tab: Tab, side: PaneSide): Entry[] {
  const pane = tab[side]
  const set = new Set(pane.selected)

  return pane.entries.filter((e) => set.has(e.path))
}

export async function createFolder(tabId: string, side: PaneSide, name: string, enter: boolean) {
  const tab = tabById(tabId)

  if (!tab || !name.trim()) {
    return
  }

  const pane = tab[side]

  try {
    if (side === 'remote') {
      if (!tab.sessionId) {
        return
      }

      const path = joinRemote(pane.path, name.trim())
      await api.remoteMkdir(tab.sessionId, path)

      if (enter) {
        await navigate(tabId, side, path)
      } else {
        await refresh(tabId, side)
        store.getState().updatePane(tabId, side, {selected: [path], cursor: path})
      }
    } else {
      const path = joinLocal(pane.path, name.trim())
      await api.localMkdir(path)

      if (enter) {
        await navigate(tabId, side, path)
      } else {
        await refresh(tabId, side)
        store.getState().updatePane(tabId, side, {selected: [path], cursor: path})
      }
    }
  } catch (error) {
    toastError(error)
  }
}

export async function createFile(tabId: string, side: PaneSide, name: string) {
  const tab = tabById(tabId)

  if (!tab || !name.trim()) {
    return
  }

  const pane = tab[side]

  try {
    if (side === 'remote') {
      if (!tab.sessionId) {
        return
      }

      await api.remoteTouch(tab.sessionId, joinRemote(pane.path, name.trim()))
    } else {
      await api.localTouch(joinLocal(pane.path, name.trim()))
    }

    await refresh(tabId, side)
  } catch (error) {
    toastError(error)
  }
}

export async function renameEntry(tabId: string, side: PaneSide, entry: Entry, newName: string) {
  const tab = tabById(tabId)

  if (!tab || !newName.trim() || newName === entry.name) {
    return
  }

  const pane = tab[side]

  try {
    if (side === 'remote') {
      if (!tab.sessionId) {
        return
      }

      await api.remoteRename(tab.sessionId, entry.path, joinRemote(pane.path, newName.trim()))
    } else {
      await api.localRename(entry.path, joinLocal(pane.path, newName.trim()))
    }

    await refresh(tabId, side)
  } catch (error) {
    toastError(error)
  }
}

export async function deleteEntries(tabId: string, side: PaneSide, entries: Entry[]) {
  const tab = tabById(tabId)

  if (!tab || entries.length === 0) {
    return
  }

  const run = async () => {
    try {
      if (side === 'remote') {
        if (!tab.sessionId) {
          return
        }

        await api.remoteDelete(
          tab.sessionId,
          entries.map((e) => ({path: e.path, isDir: e.kind === 'dir'}))
        )
      } else {
        await api.localDelete(entries.map((e) => e.path))
      }
    } catch (error) {
      toastError(error)
    }

    await refresh(tabId, side)
  }

  if (store.getState().settings.confirmDelete) {
    store.getState().openDialog({
      kind: 'confirm',
      title: t('dialog.deleteTitle'),
      message: t('dialog.deleteConfirm', {n: entries.length}),
      danger: true,
      confirmLabel: t('dialog.delete'),
      onConfirm: run
    })
  } else {
    await run()
  }
}

export async function applyPermissions(tabId: string, entries: Entry[], mode: number) {
  const tab = tabById(tabId)

  if (!tab?.sessionId) {
    return
  }

  try {
    await api.remoteChmod(
      tab.sessionId,
      entries.map((e) => e.path),
      mode
    )
    await refresh(tabId, 'remote')
  } catch (error) {
    toastError(error)
  }
}

export async function viewEdit(tabId: string, side: PaneSide, entry: Entry) {
  const tab = tabById(tabId)

  if (!tab) {
    return
  }

  try {
    if (side === 'remote') {
      if (!tab.sessionId) {
        return
      }

      await api.remoteEditOpen(tab.sessionId, entry.path)
    } else {
      await api.openPath(entry.path)
    }
  } catch (error) {
    toastError(error)
  }
}

export async function uploadEditedFile(event: EditedFileChanged) {
  const tab =
    store.getState().tabs.find((t) => t.sessionId === event.sessionId) ??
    store.getState().tabs.find((t) => t.site?.id === event.siteId && t.sessionId)

  if (!tab?.sessionId) {
    store.getState().pushToast(t('queue.waitingConnection'), 'info')

    return
  }

  await api
    .queueAdd([
      {
        sessionId: tab.sessionId,
        direction: 'upload',
        localPath: event.localPath,
        remotePath: event.remotePath,
        isDir: false,
        size: null,
        mtime: null,
        conflictPolicy: 'overwrite',
        priority: 10
      }
    ])
    .catch(toastError)
}

export async function copyUrls(tab: Tab, side: PaneSide, entries: Entry[]) {
  const text =
    side === 'remote' && tab.site
      ? entries.map((e) => buildUrl(tab.site!, e.path)).join('\n')
      : entries.map((e) => e.path).join('\n')
  await navigator.clipboard.writeText(text)
}

export async function answerPrompt(promptId: string, answer: string | null) {
  await api.promptAnswer(promptId, answer).catch(toastError)
}

export async function answerConflict(itemId: string, action: ConflictAction, applyToAll: boolean) {
  await api.conflictAnswer(itemId, action, applyToAll).catch(toastError)
}

export async function saveSettings(settings: AppSettings) {
  const previous = store.getState().settings

  try {
    await api.settingsSave(settings)
    store.getState().setSettings(settings)
    applyTheme(settings.theme)
    document.documentElement.lang = settings.language

    if (previous.showHidden !== settings.showHidden) {
      for (const tab of store.getState().tabs) {
        await refresh(tab.id, 'local')
      }
    }
  } catch (error) {
    toastError(error)
  }
}

export async function updateSettings(patch: Partial<AppSettings>) {
  await saveSettings({...store.getState().settings, ...patch})
}

export async function saveSites(tree: {root: import('../lib/api').SiteNode[]}) {
  try {
    await api.sitesSave(tree)
    store.getState().setSites(tree)
  } catch (error) {
    toastError(error)
  }
}

export async function importFileZilla() {
  const selected = await openFileDialog({
    multiple: false,
    directory: false,
    filters: [{name: 'FileZilla', extensions: ['xml']}]
  })

  if (!selected || typeof selected !== 'string') {
    return
  }

  try {
    const tree = await api.sitesImportFileZilla(selected)
    store.getState().setSites(tree)
    store.getState().pushToast(t('sm.import'), 'info')
  } catch (error) {
    toastError(error)
  }
}

export async function importWinScp() {
  try {
    const tree = await api.sitesImportWinScp()
    store.getState().setSites(tree)
    store.getState().pushToast(t('sm.import'), 'info')
  } catch (error) {
    toastError(error)
  }
}

export async function pickDirectory(initial?: string): Promise<string | null> {
  const selected = await openFileDialog({
    directory: true,
    multiple: false,
    defaultPath: initial || undefined
  })

  return typeof selected === 'string' ? selected : null
}

export async function pickFile(initial?: string): Promise<string | null> {
  const selected = await openFileDialog({
    directory: false,
    multiple: false,
    defaultPath: initial || undefined
  })

  return typeof selected === 'string' ? selected : null
}

export const queueActions = {
  pause: (id: string) => api.queuePause(id).catch(toastError),
  resume: (id: string) => api.queueResume(id).catch(toastError),
  remove: (id: string) => api.queueRemove(id).catch(toastError),
  move: (id: string, index: number) => api.queueMove(id, index).catch(toastError),
  priority: (id: string, priority: number) => api.queuePriority(id, priority).catch(toastError),
  pauseAll: () => api.queuePauseAll().catch(toastError),
  resumeAll: () => api.queueResumeAll().catch(toastError),
  retryFailed: () => api.queueRetryFailed().catch(toastError),
  removeFailed: () => api.queueRemoveFailed().catch(toastError),
  clearHistory: () => api.queueClearHistory().catch(toastError),
  clear: () => api.queueClear().catch(toastError)
}
export function clearQueue(pending: number) {
  if (pending === 0) {
    return
  }

  store.getState().openDialog({
    kind: 'confirm',
    title: t('queue.clearQueueTitle'),
    message: t('queue.clearQueueConfirm', {n: pending}),
    danger: true,
    confirmLabel: t('queue.clearQueue'),
    onConfirm: () => queueActions.clear()
  })
}