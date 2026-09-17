import type { LocalEntry, RemoteEntry } from './api'

export type EntryKind = 'file' | 'dir' | 'symlink' | 'junction' | 'shortcut' | 'drive' | 'other'

export interface Entry {
  name: string,
  path: string,
  kind: EntryKind,
  size: number,
  mtime: number | null,
  permissions: number | null,
  owner: string | null,
  group: string | null,
  linkTarget: string | null,
  linkBroken: boolean,
  targetIsDir: boolean | null,
  hidden: boolean
}

export type SortKey = 'name' | 'size' | 'mtime' | 'permissions' | 'owner' | 'target'
export type SortDirection = 'asc' | 'desc'

export interface SortSpec {
  key: SortKey,
  direction: SortDirection
}

export function fromRemote(entry: RemoteEntry): Entry {
  return {
    name: entry.name,
    path: entry.path,
    kind: entry.kind,
    size: entry.size,
    mtime: entry.mtime,
    permissions: entry.permissions,
    owner: entry.owner,
    group: entry.group,
    linkTarget: entry.linkTarget,
    linkBroken: entry.linkBroken,
    targetIsDir: entry.targetIsDir,
    hidden: entry.name.startsWith('.')
  }
}

export function fromLocal(entry: LocalEntry): Entry {
  return {
    name: entry.name,
    path: entry.path,
    kind: entry.kind,
    size: entry.size,
    mtime: entry.mtime,
    permissions: null,
    owner: null,
    group: null,
    linkTarget: entry.linkTarget,
    linkBroken: entry.linkBroken,
    targetIsDir: entry.targetIsDir,
    hidden: entry.hidden
  }
}

export function isLink(entry: Entry): boolean {
  return entry.kind === 'symlink' || entry.kind === 'junction' || entry.kind === 'shortcut'
}

export function isDirLike(entry: Entry): boolean {
  if (entry.kind === 'dir' || entry.kind === 'drive') {
    return true
  }

  return isLink(entry) && entry.targetIsDir === true
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' })

export function sortEntries(entries: Entry[], sort: SortSpec): Entry[] {
  const factor = sort.direction === 'asc' ? 1 : -1
  const compare = (a: Entry, b: Entry): number => {
    const dirA = isDirLike(a) ? 0 : 1
    const dirB = isDirLike(b) ? 0 : 1

    if (dirA !== dirB) {
      return dirA - dirB
    }

    let result = 0
    switch (sort.key) {
      case 'size':
        result = a.size - b.size
        break
      case 'mtime':
        result = (a.mtime ?? 0) - (b.mtime ?? 0)
        break
      case 'permissions':
        result = (a.permissions ?? -1) - (b.permissions ?? -1)
        break
      case 'owner':
        result = collator.compare(a.owner ?? '', b.owner ?? '')
        break
      case 'target':
        result = collator.compare(a.linkTarget ?? '', b.linkTarget ?? '')
        break
      default:
        result = 0
    }

    if (result === 0) {
      result = collator.compare(a.name, b.name)
    }

    return result * factor
  }

  return [...entries].sort(compare)
}

export function filterEntries(entries: Entry[], filter: string): Entry[] {
  const needle = filter.trim().toLowerCase()

  if (!needle) {
    return entries
  }

  return entries.filter((e) => e.name.toLowerCase().includes(needle))
}

export function totalSize(entries: Entry[]): number {
  return entries.reduce((sum, e) => (isDirLike(e) ? sum : sum + e.size), 0)
}