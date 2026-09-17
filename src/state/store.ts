import { create } from 'zustand'
import { type Language, type MessageKey, normalizeLanguage, translate } from '@/i18n'
import type {
  AppSettings,
  ConflictPrompt,
  EditedFileChanged,
  HostKeyChangedDetail,
  LogMessage,
  ProgressEvent,
  QueueSnapshot,
  QueueStats,
  SiteConfig,
  SiteTree,
  SystemInfo,
  UiPrompt,
  UntrustedCertificateDetail
} from '@/lib/api'
import type { Entry, SortSpec } from '@/lib/entries'

export type PaneSide = 'local' | 'remote'

export interface ColumnWidths {
  name: number,
  size: number,
  mtime: number,
  permissions: number,
  owner: number,
  target: number
}

export interface PaneState {
  path: string,
  entries: Entry[],
  loading: boolean,
  error: string | null,
  selected: string[],
  cursor: string | null,
  sort: SortSpec,
  history: string[],
  historyIndex: number,
  filter: string,
  columns: ColumnWidths
}

export interface Tab {
  id: string,
  sessionId: string | null,
  site: SiteConfig | null,
  connecting: boolean,
  remote: PaneState,
  local: PaneState,
  syncBrowsing: boolean,
  syncBase: { local: string, remote: string } | null
}

export type Dialog =
  | { kind: 'prompt', prompt: UiPrompt }
  | { kind: 'password', site: SiteConfig, tabId: string }
  | {
      kind: 'hostKeyChanged',
      detail: HostKeyChangedDetail,
      site: SiteConfig,
      tabId: string,
      password: string | null
    }
  | {
      kind: 'certificate',
      detail: UntrustedCertificateDetail,
      site: SiteConfig,
      tabId: string,
      password: string | null
    }
  | { kind: 'conflict', prompt: ConflictPrompt }
  | {
      kind: 'input',
      title: string,
      label: string,
      initial: string,
      selectStem?: boolean,
      onSubmit: (value: string) => void | Promise<void>
    }
  | {
      kind: 'confirm',
      title: string,
      message: string,
      danger?: boolean,
      confirmLabel?: string,
      onConfirm: () => void | Promise<void>
    }
  | { kind: 'permissions', tabId: string, entries: Entry[] }
  | { kind: 'siteManager', selectId?: string }
  | { kind: 'settings' }
  | { kind: 'sync', tabId: string }
  | { kind: 'about' }
  | { kind: 'editChanged', event: EditedFileChanged }

export interface Toast {
  id: string,
  text: string,
  kind: 'error' | 'info'
}

export type BottomTab = 'queue' | 'failed' | 'successful' | 'messages'

export const defaultColumns: ColumnWidths = {
  name: 320,
  size: 90,
  mtime: 140,
  permissions: 100,
  owner: 110,
  target: 200
}

export function createPane(path = ''): PaneState {
  return {
    path,
    entries: [],
    loading: false,
    error: null,
    selected: [],
    cursor: null,
    sort: { key: 'name', direction: 'asc' },
    history: path ? [path] : [],
    historyIndex: path ? 0 : -1,
    filter: '',
    columns: { ...defaultColumns }
  }
}

export function createTab(localPath: string): Tab {
  return {
    id: crypto.randomUUID(),
    sessionId: null,
    site: null,
    connecting: false,
    remote: createPane(''),
    local: createPane(localPath),
    syncBrowsing: false,
    syncBase: null
  }
}

export const defaultSettings: AppSettings = {
  theme: 'dark',
  language: 'pt-BR',
  defaultConflictPolicy: 'ask',
  cacheTtlSecs: 60,
  defaultLocalDir: '',
  showHidden: false,
  localPaneLeft: true,
  confirmDelete: true,
  symlinkDownload: 'follow',
  maxRetries: 3,
  retryBackoffMs: 2000,
  keepaliveSecs: 30,
  timeoutSecs: 30,
  bandwidthLimitKbps: 0,
  verifyHash: false,
  maxSuccessfulHistory: 500,
  systemIcons: true
}

const MAX_LOG_LINES = 5000

export interface AppStore {
  settings: AppSettings,
  language: Language,
  sites: SiteTree,
  system: SystemInfo | null,
  tabs: Tab[],
  activeTabId: string,
  focusedPane: PaneSide,
  queue: QueueSnapshot,
  stats: QueueStats,
  logs: LogMessage[],
  showTrace: boolean,
  dialogs: Dialog[],
  toasts: Toast[],
  bottomTab: BottomTab,
  bottomHeight: number,
  setSettings: (settings: AppSettings) => void,
  setSites: (sites: SiteTree) => void,
  setSystem: (system: SystemInfo) => void,
  addTab: (tab: Tab) => void,
  removeTab: (tabId: string) => void,
  setActiveTab: (tabId: string) => void,
  updateTab: (tabId: string, patch: Partial<Tab> | ((tab: Tab) => Partial<Tab>)) => void,
  updatePane: (
    tabId: string,
    side: PaneSide,
    patch: Partial<PaneState> | ((pane: PaneState) => Partial<PaneState>)
  ) => void,
  setFocusedPane: (side: PaneSide) => void,
  setQueue: (queue: QueueSnapshot) => void,
  setStats: (stats: QueueStats) => void,
  applyProgress: (event: ProgressEvent) => void,
  appendLog: (message: LogMessage) => void,
  clearLogs: () => void,
  setShowTrace: (value: boolean) => void,
  openDialog: (dialog: Dialog) => void,
  closeDialog: () => void,
  closeDialogWhere: (predicate: (dialog: Dialog) => boolean) => void,
  pushToast: (text: string, kind?: Toast['kind']) => void,
  dismissToast: (id: string) => void,
  setBottomTab: (tab: BottomTab) => void,
  setBottomHeight: (height: number) => void
}

export const useStore = create<AppStore>((set) => ({
  settings: defaultSettings,
  language: 'pt-BR',
  sites: { root: [] },
  system: null,
  tabs: [],
  activeTabId: '',
  focusedPane: 'remote',
  queue: { items: [], history: [] },
  stats: { active: 0, queued: 0, failed: 0, paused: 0, speedBps: 0, remainingBytes: 0 },
  logs: [],
  showTrace: false,
  dialogs: [],
  toasts: [],
  bottomTab: 'queue',
  bottomHeight: 200,
  setSettings: (settings) => set({ settings, language: normalizeLanguage(settings.language) }),
  setSites: (sites) => set({ sites }),
  setSystem: (system) => set({ system }),
  addTab: (tab) => set((state) => ({ tabs: [...state.tabs, tab], activeTabId: tab.id })),
  removeTab: (tabId) =>
    set((state) => {
      const index = state.tabs.findIndex((t) => t.id === tabId)
      const tabs = state.tabs.filter((t) => t.id !== tabId)
      let activeTabId = state.activeTabId

      if (activeTabId === tabId) {
        const next = tabs[Math.min(index, tabs.length - 1)]
        activeTabId = next ? next.id : ''
      }

      return { tabs, activeTabId }
    }),
  setActiveTab: (activeTabId) => set({ activeTabId }),
  updateTab: (tabId, patch) =>
    set((state) => ({
      tabs: state.tabs.map((tab) =>
        tab.id === tabId
          ? {
            ...tab,
            ...(typeof patch === 'function' ? patch(tab) : patch)
          }
          : tab
      )
    })),
  updatePane: (tabId, side, patch) =>
    set((state) => ({
      tabs: state.tabs.map((tab) => {
        if (tab.id !== tabId) {
          return tab
        }

        const pane = tab[side]
        const next = typeof patch === 'function' ? patch(pane) : patch

        return { ...tab, [side]: { ...pane, ...next } }
      })
    })),
  setFocusedPane: (focusedPane) => set({ focusedPane }),
  setQueue: (queue) => set({ queue }),
  setStats: (stats) => set({ stats }),
  applyProgress: (event) =>
    set((state) => {
      const index = state.queue.items.findIndex((i) => i.id === event.id)

      if (index < 0) {
        return {}
      }

      const items = state.queue.items.slice()
      const item = items[index]
      items[index] = {
        ...item,
        transferred: event.transferred,
        speedBps: event.speedBps,
        size: event.size ?? item.size
      }

      return { queue: { ...state.queue, items } }
    }),
  appendLog: (message) =>
    set((state) => {
      const logs =
        state.logs.length >= MAX_LOG_LINES
          ? state.logs.slice(-Math.floor(MAX_LOG_LINES * 0.8))
          : state.logs.slice()
      logs.push(message)

      return { logs }
    }),
  clearLogs: () => set({ logs: [] }),
  setShowTrace: (showTrace) => set({ showTrace }),
  openDialog: (dialog) => set((state) => ({ dialogs: [...state.dialogs, dialog] })),
  closeDialog: () => set((state) => ({ dialogs: state.dialogs.slice(0, -1) })),
  closeDialogWhere: (predicate) =>
    set((state) => ({
      dialogs: state.dialogs.filter((d) => !predicate(d))
    })),
  pushToast: (text, kind = 'error') =>
    set((state) => ({
      toasts: [...state.toasts, { id: crypto.randomUUID(), text, kind }].slice(-5)
    })),
  dismissToast: (id) => set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
  setBottomTab: (bottomTab) => set({ bottomTab }),
  setBottomHeight: (bottomHeight) =>
    set({ bottomHeight: Math.max(90, Math.min(600, bottomHeight)) })
}))

export function useT() {
  const language = useStore((s) => s.language)

  return (key: MessageKey, params?: Record<string, string | number>) =>
    translate(language, key, params)
}

export function t(key: MessageKey, params?: Record<string, string | number>): string {
  return translate(useStore.getState().language, key, params)
}

export function activeTab(): Tab | undefined {
  const state = useStore.getState()

  return state.tabs.find((tab) => tab.id === state.activeTabId)
}

export function tabById(tabId: string): Tab | undefined {
  return useStore.getState().tabs.find((tab) => tab.id === tabId)
}