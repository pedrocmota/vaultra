import type {Entry} from '@/lib/entries'
import {isDirLike, isLink} from '@/lib/entries'
import {iconKey, useIconStore} from '@/state/icons'
import {useStore} from '@/state/store'

const base = {
  width: 16,
  height: 16,
  viewBox: '0 0 16 16',
  fill: 'none',
  xmlns: 'http://www.w3.org/2000/svg'
} as const

export function FolderIcon() {
  return (
    <svg {...base} className="ico">
      <path
        d="M1.5 4.5A1.5 1.5 0 0 1 3 3h3.2l1.6 1.5H13a1.5 1.5 0 0 1 1.5 1.5v6A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12v-7.5Z"
        fill="#e8b84a"
      />
      <path d="M1.5 6.5h13V12A1.5 1.5 0 0 1 13 13.5H3A1.5 1.5 0 0 1 1.5 12V6.5Z" fill="#f2c85c" />
    </svg>
  )
}

export function FileIcon() {
  return (
    <svg {...base} className="ico">
      <path
        d="M3.5 2A1 1 0 0 1 4.5 1h5l3 3v9.5a1 1 0 0 1-1 1h-7a1 1 0 0 1-1-1V2Z"
        fill="var(--text-muted)"
        opacity="0.85"
      />
      <path d="M9.5 1v3h3" fill="var(--bg)" opacity="0.6" />
    </svg>
  )
}

export function DriveIcon() {
  return (
    <svg {...base} className="ico">
      <rect x="1.5" y="4" width="13" height="8" rx="1.5" fill="var(--text-muted)" />
      <circle cx="12" cy="8" r="1" fill="var(--bg)" />
    </svg>
  )
}

export function LinkIcon({broken}: {broken?: boolean}) {
  const color = broken ? 'var(--danger)' : 'var(--info)'

  return (
    <svg {...base} className="ico">
      <path
        d="M6.5 9.5 9.5 6.5M7 4.5l1-1a2.5 2.5 0 0 1 3.5 3.5l-1 1M9 11.5l-1 1a2.5 2.5 0 0 1-3.5-3.5l1-1"
        stroke={color}
        strokeWidth="1.5"
        strokeLinecap="round"
      />
    </svg>
  )
}

export function EntryIcon({entry}: {entry: Entry}) {
  const systemIcons = useStore((s) => s.settings.systemIcons)
  const key = iconKey(entry)
  const image = useIconStore((s) => (systemIcons ? s.icons[key] : null))

  if (image) {
    return <img className="ico" src={image} alt="" draggable={false} />
  }

  if (entry.kind === 'drive') {
    return <DriveIcon />
  }

  if (isLink(entry)) {
    return <LinkIcon broken={entry.linkBroken} />
  }

  if (isDirLike(entry)) {
    return <FolderIcon />
  }

  return <FileIcon />
}

export function ArrowIcon({direction}: {direction: 'up' | 'down' | 'left' | 'right'}) {
  const rotation = {up: 0, right: 90, down: 180, left: 270}[direction]

  return (
    <svg {...base} style={{transform: `rotate(${rotation}deg)`}}>
      <path
        d="M8 13V3M4 7l4-4 4 4"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}

export function RefreshIcon() {
  return (
    <svg {...base}>
      <path
        d="M13 8a5 5 0 1 1-1.5-3.6M13 3v2.5h-2.5"
        stroke="currentColor"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  )
}