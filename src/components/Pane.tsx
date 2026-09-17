import {useCallback, useEffect, useMemo, useRef, useState} from 'react'
import {paneKeymap, type PaneCommand} from '@/keybindings'
import {api} from '@/lib/api'
import {
  filterEntries,
  isDirLike,
  isLink,
  sortEntries,
  type Entry,
  type SortKey
} from '@/lib/entries'
import {isTextInput, type CommandHandlers} from '@/lib/keymap'
import {
  copyUrls,
  createFile,
  createFolder,
  deleteEntries,
  goHistory,
  goToTarget,
  goUp,
  navigate,
  openEntry,
  refresh,
  renameEntry,
  selectedEntries,
  transferEntries,
  viewEdit
} from '@/state/actions'
import {useStore, useT, type ColumnWidths, type PaneSide} from '@/state/store'
import {ContextMenu, type MenuItem, type MenuState} from './ContextMenu'
import {beginEntryDrag, FileList} from './FileList'
import {ArrowIcon, RefreshIcon} from './Icons'

const PAGE_SIZE = 20

interface Props {
  tabId: string,
  side: PaneSide,
  style?: React.CSSProperties
}

export function Pane({tabId, side, style}: Props) {
  const t = useT()
  const tab = useStore((s) => s.tabs.find((x) => x.id === tabId))
  const focused = useStore((s) => s.focusedPane === side)
  const setFocusedPane = useStore((s) => s.setFocusedPane)
  const updatePane = useStore((s) => s.updatePane)
  const openDialog = useStore((s) => s.openDialog)
  const [address, setAddress] = useState('')
  const [menu, setMenu] = useState<MenuState | null>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const pane = tab?.[side]

  useEffect(() => {
    setAddress(pane?.path ?? '')
  }, [pane?.path])

  const visible = useMemo(() => {
    if (!pane) {
      return []
    }

    return sortEntries(filterEntries(pane.entries, pane.filter), pane.sort)
  }, [pane?.entries, pane?.filter, pane?.sort])

  const isRemote = side === 'remote'
  const connected = isRemote ? Boolean(tab?.sessionId) : true

  const select = useCallback(
    (selected: string[], cursor: string | null) => updatePane(tabId, side, {selected, cursor}),
    [tabId, side, updatePane]
  )

  const currentSelection = () => (tab ? selectedEntries(tab, side) : [])

  const promptName = (
    title: string,
    initial: string,
    onSubmit: (value: string) => void | Promise<void>,
    selectStem = false
  ) => openDialog({kind: 'input', title, label: t('dialog.name'), initial, selectStem, onSubmit})

  const actions = {
    transfer: (entries: Entry[]) => transferEntries(tabId, side, entries),
    enqueue: (entries: Entry[]) => transferEntries(tabId, side, entries, {startPaused: true}),
    newFolder: (enter: boolean) =>
      promptName(t('dialog.newFolder'), '', (name) => createFolder(tabId, side, name, enter)),
    newFile: () => promptName(t('dialog.newFile'), '', (name) => createFile(tabId, side, name)),
    rename: (entry: Entry) =>
      promptName(
        t('dialog.rename'),
        entry.name,
        (name) => renameEntry(tabId, side, entry, name),
        true
      ),
    remove: (entries: Entry[]) => deleteEntries(tabId, side, entries),
    refresh: () => refresh(tabId, side)
  }

  const buildMenu = (entry: Entry | null): MenuItem[] => {
    const selection = entry
      ? pane?.selected.includes(entry.path)
        ? currentSelection()
        : [entry]
      : []

    const single = selection.length === 1 ? selection[0] : null
    const anyDir = selection.some(isDirLike)
    const transferLabel = isRemote ? t('ctx.download') : t('ctx.upload')

    if (!entry) {
      return [
        {
          label: t('ctx.newFolder'),
          shortcut: paneKeymap.label('newFolder'),
          onClick: () => actions.newFolder(false),
          disabled: !connected
        },
        {
          label: t('ctx.newFolderEnter'),
          onClick: () => actions.newFolder(true),
          disabled: !connected
        },
        {label: t('ctx.newFile'), onClick: () => actions.newFile(), disabled: !connected},
        {separator: true},
        {
          label: t('ctx.refresh'),
          shortcut: paneKeymap.label('refresh'),
          onClick: actions.refresh,
          disabled: !connected
        },
        {separator: true},
        {
          label: t('menu.selectAll'),
          shortcut: paneKeymap.label('selectAll'),
          onClick: () =>
            select(
              visible.map((e) => e.path),
              pane?.cursor ?? null
            )
        }
      ]
    }

    return [
      {
        label: transferLabel,
        shortcut: paneKeymap.label('transfer'),
        onClick: () => actions.transfer(selection),
        disabled: !tab?.sessionId
      },
      {
        label: t('ctx.addToQueue'),
        onClick: () => actions.enqueue(selection),
        disabled: !tab?.sessionId
      },
      {separator: true},
      {
        label: isRemote ? t('ctx.viewEdit') : t('ctx.open'),
        shortcut: paneKeymap.label('viewEdit'),
        disabled: !single || anyDir,
        onClick: () => single && viewEdit(tabId, side, single)
      },
      {
        label: t('ctx.enter'),
        hidden: !single || !isDirLike(single),
        onClick: () => single && openEntry(tabId, side, single)
      },
      {
        label: t('ctx.goToTarget'),
        hidden: !single || !isLink(single),
        onClick: () => single && goToTarget(tabId, side, single)
      },
      {separator: true},
      {
        label: t('ctx.newFolder'),
        shortcut: paneKeymap.label('newFolder'),
        onClick: () => actions.newFolder(false)
      },
      {label: t('ctx.newFolderEnter'), onClick: () => actions.newFolder(true)},
      {label: t('ctx.newFile'), onClick: () => actions.newFile()},
      {label: t('ctx.refresh'), shortcut: paneKeymap.label('refresh'), onClick: actions.refresh},
      {separator: true},
      {
        label: t('ctx.delete'),
        shortcut: paneKeymap.label('delete'),
        danger: true,
        onClick: () => actions.remove(selection)
      },
      {
        label: t('ctx.rename'),
        shortcut: paneKeymap.label('rename'),
        disabled: !single,
        onClick: () => single && actions.rename(single)
      },
      {separator: true},
      {
        label: isRemote ? t('ctx.copyUrl') : t('ctx.copyPath'),
        onClick: () => tab && copyUrls(tab, side, selection)
      },
      {
        label: t('ctx.openInExplorer'),
        hidden: isRemote,
        onClick: () => pane && api.openPath(pane.path)
      },
      {
        label: t('ctx.permissions'),
        hidden: !isRemote,
        onClick: () => openDialog({kind: 'permissions', tabId, entries: selection})
      }
    ]
  }

  const commandHandlers = (): CommandHandlers<PaneCommand> => {
    const index = pane?.cursor ? visible.findIndex((e) => e.path === pane.cursor) : -1
    const last = visible.length - 1
    const selection = currentSelection()
    const single = selection.length === 1 ? selection[0] : null
    const moveCursor = (next: number, extend: boolean) => {
      const entry = visible[Math.max(0, Math.min(last, next))]

      if (!entry || !pane) {
        return
      }

      select(
        extend ? Array.from(new Set([...pane.selected, entry.path])) : [entry.path],
        entry.path
      )
    }

    return {
      cursorDown: () => moveCursor(index + 1, false),
      cursorUp: () => moveCursor(index - 1, false),
      cursorFirst: () => moveCursor(0, false),
      cursorLast: () => moveCursor(last, false),
      cursorPageDown: () => moveCursor(index + PAGE_SIZE, false),
      cursorPageUp: () => moveCursor(index - PAGE_SIZE, false),
      selectDown: () => moveCursor(index + 1, true),
      selectUp: () => moveCursor(index - 1, true),
      selectToFirst: () => moveCursor(0, true),
      selectToLast: () => moveCursor(last, true),
      selectPageDown: () => moveCursor(index + PAGE_SIZE, true),
      selectPageUp: () => moveCursor(index - PAGE_SIZE, true),
      open: () => index >= 0 && openEntry(tabId, side, visible[index]),
      goUp: () => goUp(tabId, side),
      delete: () => selection.length > 0 && actions.remove(selection),
      rename: () => single && actions.rename(single),
      viewEdit: () => single && !isDirLike(single) && viewEdit(tabId, side, single),
      transfer: () => selection.length > 0 && actions.transfer(selection),
      newFolder: () => actions.newFolder(false),
      selectAll: () =>
        select(
          visible.map((e) => e.path),
          pane?.cursor ?? null
        ),
      refresh: () => actions.refresh(),
      historyBack: () => goHistory(tabId, side, -1),
      historyForward: () => goHistory(tabId, side, 1)
    }
  }

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (!pane || !tab) {
      return
    }

    paneKeymap.dispatch(event, commandHandlers(), {inInput: isTextInput(event.target)})
  }

  const onDragEntries = (event: React.MouseEvent, entries: Entry[]) => {
    const label = entries.length === 1 ? entries[0].name : t('status.items', {n: entries.length})
    let lastPane: Element | null = null
    beginEntryDrag(
      event,
      label,
      (target) => {
        lastPane?.classList.remove('drop-target')
        const paneElement = target?.closest<HTMLElement>('.pane[data-side]')

        if (!paneElement || paneElement.dataset.tab !== tabId) {
          return
        }

        const targetSide = paneElement.dataset.side as PaneSide
        const row = target?.closest<HTMLElement>('.row[data-path]')
        const destDir = row && row.dataset.dir === '1' ? row.dataset.path : undefined

        if (targetSide === side) {
          return
        }

        void transferEntries(tabId, side, entries, {destDir})
      },
      (target) => {
        const list =
          target?.closest<HTMLElement>('.pane[data-side]')?.querySelector('.filelist') ?? null

        if (list !== lastPane) {
          lastPane?.classList.remove('drop-target')

          if (list && list.closest<HTMLElement>('.pane')?.dataset.side !== side) {
            list.classList.add('drop-target')
          }

          lastPane = list
        }
      }
    )
  }

  if (!tab || !pane) {
    return null
  }

  return (
    <div
      ref={containerRef}
      className={`pane${focused ? ' focused' : ''}`}
      style={style}
      data-side={side}
      data-tab={tabId}
      tabIndex={0}
      onFocus={() => setFocusedPane(side)}
      onMouseDown={() => setFocusedPane(side)}
      onKeyDown={onKeyDown}
    >
      <div className="pane-header">
        <div className="row">
          <span className="label">{isRemote ? t('pane.remote') : t('pane.local')}</span>
          {isRemote && tab.site && (
            <span className="site-name" title={`${tab.site.host}:${tab.site.port}`}>
              {tab.site.name || tab.site.host}
            </span>
          )}
        </div>
        <div className="row">
          <button
            className="icon ghost"
            title={t('pane.back')}
            disabled={pane.historyIndex <= 0}
            onClick={() => goHistory(tabId, side, -1)}
          >
            <ArrowIcon direction="left" />
          </button>
          <button
            className="icon ghost"
            title={t('pane.forward')}
            disabled={pane.historyIndex >= pane.history.length - 1}
            onClick={() => goHistory(tabId, side, 1)}
          >
            <ArrowIcon direction="right" />
          </button>
          <button
            className="icon ghost"
            title={t('pane.up')}
            disabled={!connected}
            onClick={() => goUp(tabId, side)}
          >
            <ArrowIcon direction="up" />
          </button>
          <button
            className="icon ghost"
            title={t('ctx.refresh')}
            disabled={!connected}
            onClick={actions.refresh}
          >
            <RefreshIcon />
          </button>
          <input
            className="address"
            type="text"
            value={address}
            disabled={!connected}
            spellCheck={false}
            onChange={(e) => setAddress(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                void navigate(tabId, side, address)
                containerRef.current?.focus()
              } else if (e.key === 'Escape') {
                setAddress(pane.path)
                containerRef.current?.focus()
              }
            }}
          />
          <input
            className="filter"
            type="text"
            placeholder={t('pane.filter')}
            value={pane.filter}
            disabled={!connected}
            onChange={(e) => updatePane(tabId, side, {filter: e.target.value})}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                updatePane(tabId, side, {filter: ''})
                containerRef.current?.focus()
              }
            }}
          />
        </div>
      </div>
      <FileList
        tabId={tabId}
        side={side}
        entries={visible}
        selected={pane.selected}
        cursor={pane.cursor}
        sort={pane.sort}
        columns={pane.columns}
        loading={pane.loading || Boolean(tab.connecting && isRemote)}
        error={pane.error}
        emptyText={
          !connected
            ? t('pane.notConnected')
            : pane.path === '' && !isRemote
              ? t('pane.drives')
              : t('pane.empty')
        }
        showPermissions={isRemote}
        onOpen={(entry) => void openEntry(tabId, side, entry)}
        onSelect={select}
        onSort={(key: SortKey) =>
          updatePane(tabId, side, (p) => ({
            sort: {
              key,
              direction: p.sort.key === key && p.sort.direction === 'asc' ? 'desc' : 'asc'
            }
          }))
        }
        onResize={(column: keyof ColumnWidths, width: number) =>
          updatePane(tabId, side, (p) => ({columns: {...p.columns, [column]: width}}))
        }
        onContextMenu={(event, entry) =>
          setMenu({
            x: event.clientX,
            y: event.clientY,
            items: buildMenu(entry)
          })
        }
        onDragEntries={onDragEntries}
      />
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />
      )}
    </div>
  )
}