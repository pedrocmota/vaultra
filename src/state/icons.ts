import { create } from 'zustand'
import { api } from '@/lib/api'
import type { Entry } from '@/lib/entries'
import { isDirLike } from '@/lib/entries'

interface IconStore {
  icons: Record<string, string | null>,
  merge: (batch: Record<string, string | null>) => void
}

export const useIconStore = create<IconStore>((set) => ({
  icons: {},
  merge: (batch) => set((state) => ({ icons: { ...state.icons, ...batch } }))
}))

const pending = new Set<string>()

export function iconKey(entry: Entry): string {
  if (entry.kind === 'drive') {
    return `drive:${entry.path}`
  }

  if (isDirLike(entry)) {
    return 'folder'
  }

  const dot = entry.name.lastIndexOf('.')
  const extension = dot > 0 ? entry.name.slice(dot + 1).toLowerCase() : ''

  return `ext:${extension}`
}

export async function requestIcons(entries: Entry[]) {
  const known = useIconStore.getState().icons
  const keys = Array.from(new Set(entries.map(iconKey))).filter(
    (key) => !(key in known) && !pending.has(key)
  )

  if (keys.length === 0) {
    return
  }

  keys.forEach((key) => pending.add(key))

  try {
    const batch = await api.fileIcons(keys)
    useIconStore.getState().merge(batch)
  } catch {
    useIconStore.getState().merge(Object.fromEntries(keys.map((key) => [key, null])))
  } finally {
    keys.forEach((key) => pending.delete(key))
  }
}