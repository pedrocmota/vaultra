import {invoke} from '@tauri-apps/api/core'

export type Protocol = 'ftp' | 'ftps_explicit' | 'ftps_implicit' | 'sftp'
export type LogonType = 'anonymous' | 'normal' | 'ask' | 'interactive' | 'key_file'
export type ServerType = 'auto' | 'unix' | 'windows'
export type TransferMode = 'default' | 'passive' | 'active'
export type EncodingMode = {mode: 'auto'} | {mode: 'utf8'} | {mode: 'custom', label: string}
export type ConflictPolicy =
  'ask' | 'overwrite' | 'skip' | 'rename' | 'resume' | 'overwrite_if_newer'
export type ConflictAction = Exclude<ConflictPolicy, 'ask'>
export type Theme = 'dark' | 'light' | 'system'
export type SymlinkDownload = 'follow' | 'copy_link'
export type Direction = 'download' | 'upload' | 'copy'
export type TransferStatus = 'queued' | 'active' | 'paused' | 'failed' | 'done' | 'skipped'
export type PromptKind = 'secret' | 'confirm' | 'host_key' | 'info'
export type LogKind = 'status' | 'command' | 'response' | 'error' | 'trace'
export type RemoteKind = 'file' | 'dir' | 'symlink' | 'other'
export type LocalKind = 'file' | 'dir' | 'symlink' | 'junction' | 'shortcut' | 'drive'
export type DiffKind =
  'equal' | 'local_newer' | 'remote_newer' | 'local_only' | 'remote_only' | 'different'

export interface SftpSettings {
  keyPath: string,
  useAgent: boolean,
  proxyJump: string,
  extraOptions: string[]
}

export interface SiteConfig {
  id: string,
  name: string,
  protocol: Protocol,
  host: string,
  port: number,
  logonType: LogonType,
  user: string,
  color: string | null,
  comments: string,
  localDir: string,
  remoteDir: string,
  syncBrowsing: boolean,
  serverType: ServerType,
  bypassProxy: boolean,
  transferMode: TransferMode,
  asciiMode: boolean,
  maxConnections: number,
  conflictPolicy: ConflictPolicy | null,
  encoding: EncodingMode,
  sftp: SftpSettings,
  keepaliveSecs: number,
  timeoutSecs: number
}

export interface Bookmark {
  id: string,
  name: string,
  localDir: string,
  remoteDir: string,
  syncBrowsing: boolean
}

export type SiteNode =
  | {kind: 'folder', id: string, name: string, children: SiteNode[]}
  | {kind: 'site', site: SiteConfig, bookmarks: Bookmark[]}

export interface SiteTree {
  root: SiteNode[]
}

export interface RecentConnection {
  site: SiteConfig,
  lastUsed: number
}

export interface RemoteEntry {
  name: string,
  path: string,
  kind: RemoteKind,
  size: number,
  mtime: number | null,
  permissions: number | null,
  owner: string | null,
  group: string | null,
  linkTarget: string | null,
  linkBroken: boolean,
  targetIsDir: boolean | null
}

export interface LocalEntry {
  name: string,
  path: string,
  kind: LocalKind,
  size: number,
  mtime: number | null,
  hidden: boolean,
  readonly: boolean,
  linkTarget: string | null,
  linkBroken: boolean,
  targetIsDir: boolean | null
}

export interface Listing {
  path: string,
  entries: RemoteEntry[]
}

export interface LocalListing {
  path: string,
  parent: string | null,
  entries: LocalEntry[]
}

export interface LinkTarget {
  target: string,
  isDir: boolean
}

export interface SessionInfo {
  id: string,
  site: SiteConfig,
  initialDir: string,
  supportedHashes: string[]
}

export interface AppSettings {
  theme: Theme,
  language: string,
  defaultConflictPolicy: ConflictPolicy,
  cacheTtlSecs: number,
  defaultLocalDir: string,
  showHidden: boolean,
  localPaneLeft: boolean,
  confirmDelete: boolean,
  symlinkDownload: SymlinkDownload,
  maxRetries: number,
  retryBackoffMs: number,
  keepaliveSecs: number,
  timeoutSecs: number,
  bandwidthLimitKbps: number,
  verifyHash: boolean,
  maxSuccessfulHistory: number,
  systemIcons: boolean
}

export interface TransferItem {
  id: string,
  sessionId: string | null,
  siteId: string,
  siteName: string,
  direction: Direction,
  localPath: string,
  remotePath: string,
  isDir: boolean,
  size: number | null,
  sourceMtime: number | null,
  transferred: number,
  status: TransferStatus,
  priority: number,
  error: string | null,
  attempts: number,
  createdAt: number,
  finishedAt: number | null,
  speedBps: number,
  followSymlink: boolean,
  conflictPolicy: ConflictPolicy | null,
  linkTarget: string | null,
  batch: string | null,
  targetSessionId: string | null,
  targetPath: string | null
}

export interface QueueSnapshot {
  items: TransferItem[],
  history: TransferItem[]
}

export interface QueueStats {
  active: number,
  queued: number,
  failed: number,
  paused: number,
  speedBps: number,
  remainingBytes: number
}

export interface QueueRequest {
  sessionId: string,
  direction: Direction,
  localPath: string,
  remotePath: string,
  isDir: boolean,
  size: number | null,
  mtime: number | null,
  priority?: number,
  followSymlink?: boolean,
  conflictPolicy?: ConflictPolicy | null,
  startPaused?: boolean,
  linkTarget?: string | null,
  batch?: string | null,
  targetSessionId?: string | null,
  targetPath?: string | null
}

export interface ClipboardFiles {
  sequence: number,
  paths: string[],
  cut: boolean,
  token: string | null
}

export interface CopyItem {
  source: string,
  destination: string
}

export interface DragOutRequest {
  localPaths?: string[],
  downloads?: QueueRequest[]
}

export type DragOutcome = 'dropped' | 'cancelled'
export type DropEffect = 'none' | 'copy' | 'move' | 'link'

export interface DragOutResult {
  outcome: DragOutcome,
  effect: DropEffect
}

export interface ProgressEvent {
  id: string,
  transferred: number,
  speedBps: number,
  size: number | null
}

export interface ConflictPrompt {
  itemId: string,
  sessionId: string,
  direction: Direction,
  sourcePath: string,
  sourceSize: number,
  sourceMtime: number | null,
  targetPath: string,
  targetSize: number,
  targetMtime: number | null
}

export interface UiPrompt {
  promptId: string,
  sessionId: string,
  kind: PromptKind,
  text: string,
  host: string | null,
  keyType: string | null,
  fingerprint: string | null
}

export interface LogMessage {
  sessionId: string | null,
  kind: LogKind,
  text: string,
  ts: number
}

export interface SystemInfo {
  version: string,
  openssh: string | null,
  homeDir: string,
  dataDir: string,
  logDir: string
}

export interface DiffEntry {
  relativePath: string,
  isDir: boolean,
  kind: DiffKind,
  localSize: number | null,
  remoteSize: number | null,
  localMtime: number | null,
  remoteMtime: number | null,
  localPath: string,
  remotePath: string
}

export interface SyncRequest {
  sessionId: string,
  localDir: string,
  remoteDir: string,
  excludes: string[]
}

export interface EditedFileChanged {
  sessionId: string,
  siteId: string,
  remotePath: string,
  localPath: string
}

export interface HostKeyChangedDetail {
  host: string,
  fingerprint: string,
  keyType: string,
  knownHostsLine: string
}

export interface UntrustedCertificateDetail {
  host: string,
  fingerprint: string,
  subject: string,
  issuer: string,
  notBefore: string,
  notAfter: string,
  reason: string
}

export type BackendError =
  | {code: 'hostKeyChanged', detail: HostKeyChangedDetail}
  | {code: 'untrustedCertificate', detail: UntrustedCertificateDetail}
  | {code: 'openSshMissing' | 'invalidSession' | 'cancelled', detail?: undefined}
  | {code: string, detail: string}

export function isBackendError(value: unknown): value is BackendError {
  return typeof value === 'object' && value !== null && 'code' in value
}

export function isHostKeyChanged(
  value: unknown
): value is {code: 'hostKeyChanged', detail: HostKeyChangedDetail} {
  return (
    isBackendError(value) && value.code === 'hostKeyChanged' && typeof value.detail === 'object'
  )
}

export function isUntrustedCertificate(
  value: unknown
): value is {code: 'untrustedCertificate', detail: UntrustedCertificateDetail} {
  return (
    isBackendError(value) &&
    value.code === 'untrustedCertificate' &&
    typeof value.detail === 'object'
  )
}

export function describeError(error: unknown): string {
  if (isBackendError(error)) {
    if (typeof error.detail === 'string') {
      return error.detail
    }

    if (error.code === 'hostKeyChanged') {
      return 'Host key changed'
    }

    if (error.code === 'untrustedCertificate') {
      return 'Untrusted TLS certificate'
    }

    if (error.code === 'openSshMissing') {
      return 'OpenSSH client not found'
    }

    if (error.code === 'invalidSession') {
      return 'Session is not connected'
    }

    return error.code
  }

  if (error instanceof Error) {
    return error.message
  }

  return String(error)
}

export function defaultSite(overrides: Partial<SiteConfig> = {}): SiteConfig {
  return {
    id: crypto.randomUUID(),
    name: '',
    protocol: 'sftp',
    host: '',
    port: 22,
    logonType: 'normal',
    user: '',
    color: null,
    comments: '',
    localDir: '',
    remoteDir: '',
    syncBrowsing: false,
    serverType: 'auto',
    bypassProxy: false,
    transferMode: 'default',
    asciiMode: false,
    maxConnections: 2,
    conflictPolicy: null,
    encoding: {mode: 'auto'},
    sftp: {keyPath: '', useAgent: true, proxyJump: '', extraOptions: []},
    keepaliveSecs: 30,
    timeoutSecs: 30,
    ...overrides
  }
}

export function defaultPort(protocol: Protocol): number {
  switch (protocol) {
    case 'ftp':
    case 'ftps_explicit':
      return 21
    case 'ftps_implicit':
      return 990
    default:
      return 22
  }
}

export const api = {
  systemInfo: () => invoke<SystemInfo>('system_info'),
  settingsGet: () => invoke<AppSettings>('settings_get'),
  settingsSave: (settings: AppSettings) => invoke<void>('settings_save', {settings}),
  sitesGet: () => invoke<SiteTree>('sites_get'),
  sitesSave: (tree: SiteTree) => invoke<void>('sites_save', {tree}),
  sitePasswordGet: (siteId: string) => invoke<string | null>('site_password_get', {siteId}),
  sitePasswordSet: (siteId: string, user: string, password: string) =>
    invoke<void>('site_password_set', {siteId, user, password}),
  sitesImportFileZilla: (path: string) => invoke<SiteTree>('sites_import_filezilla', {path}),
  sitesImportWinScp: () => invoke<SiteTree>('sites_import_winscp'),
  recentGet: () => invoke<RecentConnection[]>('recent_get'),
  recentClear: () => invoke<void>('recent_clear'),
  sessionConnect: (
    site: SiteConfig,
    password: string | null,
    acceptNewHostkey: boolean,
    remember: boolean
  ) => invoke<SessionInfo>('session_connect', {site, password, acceptNewHostkey, remember}),
  sessionDisconnect: (sessionId: string) => invoke<void>('session_disconnect', {sessionId}),
  promptAnswer: (promptId: string, answer: string | null) =>
    invoke<boolean>('prompt_answer', {promptId, answer}),
  hostkeyForget: (host: string, port: number) => invoke<void>('hostkey_forget', {host, port}),
  certificateTrust: (fingerprint: string, remember: boolean) =>
    invoke<void>('certificate_trust', {fingerprint, remember}),
  remoteList: (sessionId: string, path: string, force = false) =>
    invoke<Listing>('remote_list', {sessionId, path, force}),
  remoteRealpath: (sessionId: string, path: string) =>
    invoke<string>('remote_realpath', {sessionId, path}),
  remoteStat: (sessionId: string, path: string) =>
    invoke<RemoteEntry>('remote_stat', {sessionId, path}),
  remoteMkdir: (sessionId: string, path: string) =>
    invoke<void>('remote_mkdir', {sessionId, path}),
  remoteTouch: (sessionId: string, path: string) =>
    invoke<void>('remote_touch', {sessionId, path}),
  remoteRename: (sessionId: string, from: string, to: string) =>
    invoke<void>('remote_rename', {sessionId, from, to}),
  remoteChmod: (sessionId: string, paths: string[], mode: number) =>
    invoke<void>('remote_chmod', {sessionId, paths, mode}),
  remoteDelete: (sessionId: string, targets: {path: string, isDir: boolean}[]) =>
    invoke<void>('remote_delete', {sessionId, targets}),
  remoteResolveLink: (sessionId: string, path: string) =>
    invoke<LinkTarget>('remote_resolve_link', {sessionId, path}),
  remoteEditOpen: (sessionId: string, path: string) =>
    invoke<string>('remote_edit_open', {sessionId, path}),
  localList: (path: string, showHidden?: boolean) =>
    invoke<LocalListing>('local_list', {path, showHidden}),
  localHome: () => invoke<string>('local_home'),
  localMkdir: (path: string) => invoke<void>('local_mkdir', {path}),
  localTouch: (path: string) => invoke<void>('local_touch', {path}),
  localRename: (from: string, to: string) => invoke<void>('local_rename', {from, to}),
  localDelete: (paths: string[]) => invoke<void>('local_delete', {paths}),
  localResolveLink: (path: string) => invoke<LinkTarget>('local_resolve_link', {path}),
  localCopy: (items: CopyItem[], overwrite: boolean) =>
    invoke<void>('local_copy', {items, overwrite}),
  clipboardWrite: (text: string, files: string[] | null, token: string) =>
    invoke<number>('clipboard_write', {text, files, token}),
  clipboardReadFiles: () => invoke<ClipboardFiles>('clipboard_read_files'),
  dragOut: (request: DragOutRequest) => invoke<DragOutResult>('drag_out', {request}),
  queueAdd: (requests: QueueRequest[]) => invoke<string[]>('queue_add', {requests}),
  queueSnapshot: () => invoke<QueueSnapshot>('queue_snapshot'),
  queueStats: () => invoke<QueueStats>('queue_stats'),
  queuePause: (id: string) => invoke<void>('queue_pause', {id}),
  queueResume: (id: string) => invoke<void>('queue_resume', {id}),
  queueRemove: (id: string) => invoke<void>('queue_remove', {id}),
  queueMove: (id: string, index: number) => invoke<void>('queue_move', {id, index}),
  queuePriority: (id: string, priority: number) => invoke<void>('queue_priority', {id, priority}),
  queuePauseAll: () => invoke<void>('queue_pause_all'),
  queueResumeAll: () => invoke<void>('queue_resume_all'),
  queueRetryFailed: () => invoke<void>('queue_retry_failed'),
  queueRemoveFailed: () => invoke<void>('queue_remove_failed'),
  queueClear: () => invoke<void>('queue_clear'),
  queueClearHistory: () => invoke<void>('queue_clear_history'),
  conflictAnswer: (itemId: string, action: ConflictAction, applyToAll: boolean) =>
    invoke<boolean>('conflict_answer', {itemId, answer: {action, applyToAll}}),
  syncCompare: (request: SyncRequest) => invoke<DiffEntry[]>('sync_compare', {request}),
  openPath: (path: string) => invoke<void>('open_path', {path}),
  fileIcons: (keys: string[]) => invoke<Record<string, string | null>>('file_icons', {keys})
}