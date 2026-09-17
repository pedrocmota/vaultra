import {useCallback, useEffect, useRef, useState} from 'react'
import {useVirtualizer} from '@tanstack/react-virtual'
import {startMouseDrag} from '@/lib/drag'
import type {Entry, SortKey, SortSpec} from '@/lib/entries'
import {isDirLike, isLink} from '@/lib/entries'
import {formatBytes, formatDate, formatPermissions} from '@/lib/format'
import {requestIcons} from '@/state/icons'
import {useStore, useT, type ColumnWidths, type PaneSide} from '@/state/store'
import {EntryIcon} from './Icons'

interface Props {
  tabId: string,
  side: PaneSide,
  entries: Entry[],
  selected: string[],
  cursor: string | null,
  sort: SortSpec,
  columns: ColumnWidths,
  loading: boolean,
  error: string | null,
  emptyText: string,
  showPermissions: boolean,
  onOpen: (entry: Entry) => void,
  onSelect: (selected: string[], cursor: string | null) => void,
  onSort: (key: SortKey) => void,
  onResize: (column: keyof ColumnWidths, width: number) => void,
  onContextMenu: (event: React.MouseEvent, entry: Entry | null) => void,
  onDragEntries: (event: React.MouseEvent, entries: Entry[]) => void
}

const ROW_HEIGHT = 24
const DRAG_THRESHOLD = 3
const AUTO_SCROLL_INTERVAL_MS = 50

interface Box {
  top: number,
  left: number,
  width: number,
  height: number
}

const COLUMN_ORDER: {
  key: keyof ColumnWidths,
  sort: SortKey,
  label: 'col.name' | 'col.size' | 'col.modified' | 'col.permissions' | 'col.owner' | 'col.target'
}[] = [
  {key: 'name', sort: 'name', label: 'col.name'},
  {key: 'size', sort: 'size', label: 'col.size'},
  {key: 'mtime', sort: 'mtime', label: 'col.modified'},
  {key: 'permissions', sort: 'permissions', label: 'col.permissions'},
  {key: 'owner', sort: 'owner', label: 'col.owner'},
  {key: 'target', sort: 'target', label: 'col.target'}
]

export function FileList(props: Props) {
  const {
    entries,
    selected,
    cursor,
    sort,
    columns,
    onOpen,
    onSelect,
    onSort,
    onResize,
    onContextMenu,
    onDragEntries
  } = props

  const t = useT()
  const language = useStore((s) => s.language)
  const bodyRef = useRef<HTMLDivElement>(null)
  const anchorRef = useRef<string | null>(null)
  const selectedSet = new Set(selected)
  const [marquee, setMarquee] = useState<Box | null>(null)

  const virtualizer = useVirtualizer({
    count: entries.length,
    getScrollElement: () => bodyRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 24
  })

  const systemIcons = useStore((s) => s.settings.systemIcons)
  useEffect(() => {
    if (systemIcons) {
      void requestIcons(entries)
    }
  }, [entries, systemIcons])

  useEffect(() => {
    if (!cursor) {
      return
    }

    const index = entries.findIndex((e) => e.path === cursor)

    if (index >= 0) {
      virtualizer.scrollToIndex(index, {align: 'auto'})
    }
  }, [cursor, entries, virtualizer])

  const handleClick = useCallback(
    (event: React.MouseEvent, entry: Entry) => {
      if (event.shiftKey && anchorRef.current) {
        const start = entries.findIndex((e) => e.path === anchorRef.current)
        const end = entries.findIndex((e) => e.path === entry.path)

        if (start >= 0 && end >= 0) {
          const [from, to] = start < end ? [start, end] : [end, start]
          const range = entries.slice(from, to + 1).map((e) => e.path)
          onSelect(event.ctrlKey ? Array.from(new Set([...selected, ...range])) : range, entry.path)

          return
        }
      }

      if (event.ctrlKey) {
        const next = selectedSet.has(entry.path)
          ? selected.filter((p) => p !== entry.path)
          : [...selected, entry.path]
        onSelect(next, entry.path)
      } else {
        onSelect([entry.path], entry.path)
      }

      anchorRef.current = entry.path
    },
    [entries, selected, selectedSet, onSelect]
  )

  const handleMouseDown = (event: React.MouseEvent, entry: Entry) => {
    if (event.button === 2) {
      if (!selectedSet.has(entry.path)) {
        onSelect([entry.path], entry.path)
        anchorRef.current = entry.path
      }

      return
    }

    if (event.button !== 0 || event.ctrlKey || event.shiftKey) {
      return
    }

    if (!selectedSet.has(entry.path)) {
      return
    }

    const dragged = entries.filter((e) => selectedSet.has(e.path))
    onDragEntries(event, dragged)
  }


  const startMarquee = (event: React.MouseEvent) => {
    const body = bodyRef.current

    if (!body) {
      return
    }

    event.preventDefault()
    const additive = event.ctrlKey
    const base = additive ? selected : []
    const rect = body.getBoundingClientRect()
    const origin = {
      x: event.clientX - rect.left + body.scrollLeft,
      y: event.clientY - rect.top + body.scrollTop
    }

    let pointer = {x: event.clientX, y: event.clientY}
    let dragging = false

    const update = () => {
      const current = body.getBoundingClientRect()
      const x = pointer.x - current.left + body.scrollLeft
      const y = pointer.y - current.top + body.scrollTop

      if (
        !dragging
        && Math.abs(
          x - origin.x
        ) < DRAG_THRESHOLD
        && Math.abs(
          y - origin.y
        ) < DRAG_THRESHOLD
      ) {
        return
      }

      dragging = true
      const limit = Math.max(body.scrollHeight, body.clientHeight) - 1
      const top = Math.max(0, Math.min(y, origin.y))
      const bottom = Math.min(limit, Math.max(y, origin.y))
      setMarquee(
        {top, left: Math.min(x, origin.x), width: Math.abs(x - origin.x), height: bottom - top}
      )
      const first = Math.floor(top / ROW_HEIGHT)
      const last = Math.min(entries.length - 1, Math.floor(bottom / ROW_HEIGHT))
      const inside = first <= last ? entries.slice(first, last + 1).map((e) => e.path) : []
      onSelect(Array.from(new Set([...base, ...inside])), inside[inside.length - 1] ?? cursor)
    }

    const autoScroll = () => {
      const current = body.getBoundingClientRect()

      if (pointer.y > current.bottom) {
        body.scrollTop += (pointer.y - current.bottom) / 2
      } else if (pointer.y < current.top) {
        body.scrollTop -= (current.top - pointer.y) / 2
      } else {
        return
      }

      update()
    }

    const move = (e: MouseEvent) => {
      pointer = {x: e.clientX, y: e.clientY}
      update()
    }

    const timer = window.setInterval(autoScroll, AUTO_SCROLL_INTERVAL_MS)
    const up = () => {
      window.clearInterval(timer)
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
      setMarquee(null)

      if (!dragging && !additive) {
        onSelect([], cursor)
      }
    }

    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }

  const visibleColumns = COLUMN_ORDER.filter((c) =>
    c.key === 'permissions' || c.key === 'owner' ? props.showPermissions : true
  )

  const totalWidth = visibleColumns.reduce((sum, c) => sum + columns[c.key], 0)

  const startResize = (event: React.MouseEvent, column: keyof ColumnWidths) => {
    event.preventDefault()
    event.stopPropagation()
    const startX = event.clientX
    const startWidth = columns[column]
    const move = (e: MouseEvent) => onResize(column, Math.max(50, startWidth + e.clientX - startX))
    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }

  return (
    <div className="filelist">
      {props.loading && <div className="loading-bar" />}
      <div className="header" style={{minWidth: totalWidth}}>
        {visibleColumns.map((column) => (
          <div
            key={column.key}
            className="cell"
            style={{width: columns[column.key]}}
            onClick={() => onSort(column.sort)}
          >
            {t(column.label)}
            {sort.key === column.sort && (sort.direction === 'asc' ? ' ▲' : ' ▼')}
            <div className="resizer" onMouseDown={(e) => startResize(e, column.key)} />
          </div>
        ))}
      </div>
      <div
        ref={bodyRef}
        className="body"
        onContextMenu={(e) => {
          if ((e.target as HTMLElement).closest('.row')) {
            return
          }

          e.preventDefault()
          onContextMenu(e, null)
        }}
        onMouseDown={(e) => {
          if (e.button === 0 && !(e.target as HTMLElement).closest('.row')) {
            startMarquee(e)
          }
        }}
      >
        {props.error ? (
          <div className="empty" style={{color: 'var(--danger)'}}>
            {props.error}
          </div>
        ) : entries.length === 0 && !props.loading ? (
          <div className="empty">{props.emptyText}</div>
        ) : (
          <div
            style={{
              height: virtualizer.getTotalSize(),
              position: 'relative',
              minWidth: totalWidth
            }}
          >
            {marquee && <div className="marquee" style={marquee} />}
            {virtualizer.getVirtualItems().map((row) => {
              const entry = entries[row.index]
              const link = isLink(entry)
              const classes = ['row', row.index % 2 === 1 ? 'odd' : 'even']

              if (selectedSet.has(entry.path)) {
                classes.push('selected')
              }

              if (cursor === entry.path) {
                classes.push('cursor')
              }

              if (entry.linkBroken) {
                classes.push('broken')
              }

              if (entry.hidden) {
                classes.push('hidden-entry')
              }

              const dirLike = isDirLike(entry)

              return (
                <div
                  key={entry.path}
                  className={classes.join(' ')}
                  data-path={entry.path}
                  data-dir={dirLike ? '1' : '0'}
                  style={{transform: `translateY(${row.start}px)`}}
                  title={
                    link && entry.linkTarget
                      ? `→ ${entry.linkTarget}${entry.linkBroken ? ` (${t('kind.broken')})` : ''}`
                      : undefined
                  }
                  onMouseDown={(e) => handleMouseDown(e, entry)}
                  onClick={(e) => handleClick(e, entry)}
                  onDoubleClick={() => onOpen(entry)}
                  onContextMenu={(e) => {
                    e.preventDefault()
                    onContextMenu(e, entry)
                  }}
                >
                  <div className="cell name" style={{width: columns.name}}>
                    <EntryIcon entry={entry} />
                    <span style={{overflow: 'hidden', textOverflow: 'ellipsis'}}>
                      {entry.name}
                    </span>
                    {link && (
                      <span className="link-badge">
                        {entry.kind === 'shortcut'
                          ? 'lnk'
                          : entry.kind === 'junction'
                            ? 'jct'
                            : '→'}
                      </span>
                    )}
                  </div>
                  <div className="cell num" style={{width: columns.size}}>
                    {dirLike ? '' : formatBytes(entry.size)}
                  </div>
                  <div className="cell" style={{width: columns.mtime}}>
                    {formatDate(entry.mtime, language)}
                  </div>
                  {props.showPermissions && (
                    <>
                      <div className="cell mono" style={{width: columns.permissions}}>
                        {formatPermissions(entry.permissions, entry.kind === 'dir')}
                      </div>
                      <div className="cell" style={{width: columns.owner}}>
                        {entry.owner
                          ? entry.group
                            ? `${entry.owner} ${entry.group}`
                            : entry.owner
                          : ''}
                      </div>
                    </>
                  )}
                  <div className="cell mono" style={{width: columns.target}}>
                    {entry.linkTarget ?? ''}
                  </div>
                </div>
              )
            })}
          </div>
        )}
      </div>
    </div>
  )
}

export function beginEntryDrag(
  event: React.MouseEvent,
  label: string,
  onDrop: (target: Element | null) => void,
  onHover?: (target: Element | null) => void
) {
  startMouseDrag(event, {label, onDrop: (target) => onDrop(target), onMove: onHover})
}