import { useEffect, useMemo, useState } from 'react'
import { Modal } from '@/components/Modal'
import {
  api,
  defaultPort,
  defaultSite,
  type Bookmark,
  type LogonType,
  type Protocol,
  type ServerType,
  type SiteConfig,
  type SiteNode,
  type SiteTree,
  type TransferMode
} from '@/lib/api'
import { startMouseDrag } from '@/lib/drag'
import {
  connectSite,
  importFileZilla,
  importWinScp,
  pickDirectory,
  pickFile,
  saveSites,
  toastError
} from '@/state/actions'
import { useStore, useT } from '@/state/store'
import { ConflictPolicySelect } from './SettingsDialog'

type Selection =
  | { kind: 'folder', id: string }
  | { kind: 'site', id: string }
  | { kind: 'bookmark', siteId: string, id: string }
  | null
type SubTab = 'general' | 'advanced' | 'transfer' | 'charset' | 'sftp'

const COLORS = [
  '#e5484d',
  '#f76b15',
  '#f5d90a',
  '#30a46c',
  '#12a594',
  '#3e63dd',
  '#8e4ec6',
  '#d6409f'
]

function cloneTree(tree: SiteTree): SiteTree {
  return JSON.parse(JSON.stringify(tree))
}

function walk(
  nodes: SiteNode[],
  visit: (node: SiteNode, parent: SiteNode[], index: number) => boolean | void
): boolean {
  for (let index = 0; index < nodes.length; index += 1) {
    const node = nodes[index]

    if (visit(node, nodes, index)) {
      return true
    }

    if (node.kind === 'folder' && walk(node.children, visit)) {
      return true
    }
  }

  return false
}

function findSiteNode(tree: SiteTree, id: string): Extract<SiteNode, { kind: 'site' }> | null {
  let found: Extract<SiteNode, { kind: 'site' }> | null = null
  walk(tree.root, (node) => {
    if (node.kind === 'site' && node.site.id === id) {
      found = node

      return true
    }
  })

  return found
}

function findFolder(tree: SiteTree, id: string): Extract<SiteNode, { kind: 'folder' }> | null {
  let found: Extract<SiteNode, { kind: 'folder' }> | null = null
  walk(tree.root, (node) => {
    if (node.kind === 'folder' && node.id === id) {
      found = node

      return true
    }
  })

  return found
}

function detach(tree: SiteTree, id: string): SiteNode | null {
  let removed: SiteNode | null = null
  walk(tree.root, (node, parent, index) => {
    const nodeId = node.kind === 'folder' ? node.id : node.site.id

    if (nodeId === id) {
      removed = parent.splice(index, 1)[0]

      return true
    }
  })

  return removed
}

function containsFolder(node: SiteNode, folderId: string): boolean {
  if (node.kind !== 'folder') {
    return false
  }

  if (node.id === folderId) {
    return true
  }

  return node.children.some((child) => containsFolder(child, folderId))
}

export function SiteManager({ selectId, close }: { selectId?: string, close: () => void }) {
  const t = useT()
  const sites = useStore((s) => s.sites)
  const activeTabId = useStore((s) => s.activeTabId)
  const [tree, setTree] = useState<SiteTree>(() => cloneTree(sites))
  const [selection, setSelection] = useState<Selection>(
    selectId ? { kind: 'site', id: selectId } : null
  )

  const [expanded, setExpanded] = useState<Set<string>>(
    () =>
      new Set(
        sites.root.filter((n) => n.kind === 'folder').map((n) => (n.kind === 'folder' ? n.id : ''))
      )
  )

  const [subTab, setSubTab] = useState<SubTab>('general')
  const [passwords, setPasswords] = useState<Record<string, string>>({})
  const [storedPassword, setStoredPassword] = useState<Record<string, boolean>>({})
  const [dropTarget, setDropTarget] = useState<string | null>(null)

  useEffect(() => {
    setTree(cloneTree(sites))
  }, [sites])

  const selectedSite = useMemo(() => {
    if (selection?.kind === 'site') {
      return findSiteNode(tree, selection.id)
    }

    if (selection?.kind === 'bookmark') {
      return findSiteNode(tree, selection.siteId)
    }

    return null
  }, [tree, selection])

  const selectedBookmark = useMemo(() => {
    if (selection?.kind !== 'bookmark' || !selectedSite) {
      return null
    }

    return selectedSite.bookmarks.find((b) => b.id === selection.id) ?? null
  }, [selection, selectedSite])

  useEffect(() => {
    if (!selectedSite) {
      return
    }

    const id = selectedSite.site.id

    if (selectedSite.site.logonType === 'normal' && storedPassword[id] === undefined) {
      api
        .sitePasswordGet(id)
        .then((value) => setStoredPassword((s) => ({ ...s, [id]: Boolean(value) })))
        .catch(() => undefined)
    }
  }, [selectedSite, storedPassword])

  useEffect(() => {
    if (selectedSite && selectedSite.site.protocol !== 'sftp' && subTab === 'sftp') {
      setSubTab('general')
    }
  }, [selectedSite, subTab])

  const mutate = (fn: (draft: SiteTree) => void) => {
    setTree((current) => {
      const draft = cloneTree(current)
      fn(draft)

      return draft
    })
  }

  const updateSite = (patch: Partial<SiteConfig>) => {
    if (!selectedSite) {
      return
    }

    const id = selectedSite.site.id
    mutate((draft) => {
      const node = findSiteNode(draft, id)

      if (node) {
        Object.assign(node.site, patch)
      }
    })
  }

  const updateBookmark = (patch: Partial<Bookmark>) => {
    if (!selectedSite || !selectedBookmark) {
      return
    }

    const siteId = selectedSite.site.id
    const bookmarkId = selectedBookmark.id
    mutate((draft) => {
      const node = findSiteNode(draft, siteId)
      const bookmark = node?.bookmarks.find((b) => b.id === bookmarkId)

      if (bookmark) {
        Object.assign(bookmark, patch)
      }
    })
  }

  const containerOf = (draft: SiteTree): SiteNode[] => {
    if (selection?.kind === 'folder') {
      return findFolder(draft, selection.id)?.children ?? draft.root
    }

    if (selection?.kind === 'site' || selection?.kind === 'bookmark') {
      const siteId = selection.kind === 'site' ? selection.id : selection.siteId
      let parent: SiteNode[] = draft.root
      walk(draft.root, (node, list) => {
        if (node.kind === 'site' && node.site.id === siteId) {
          parent = list

          return true
        }
      })

      return parent
    }

    return draft.root
  }

  const addSite = () => {
    const site = defaultSite({ name: t('sm.newSite') })
    mutate((draft) => containerOf(draft).push({ kind: 'site', site, bookmarks: [] }))
    setSelection({ kind: 'site', id: site.id })
    setSubTab('general')
  }

  const addFolder = () => {
    const id = crypto.randomUUID()
    mutate((draft) =>
      containerOf(draft).push({ kind: 'folder', id, name: t('sm.newFolder'), children: [] })
    )
    setSelection({ kind: 'folder', id })
    setExpanded((s) => new Set(s).add(id))
  }

  const addBookmark = () => {
    if (!selectedSite) {
      return
    }

    const siteId = selectedSite.site.id
    const bookmark: Bookmark = {
      id: crypto.randomUUID(),
      name: t('sm.newBookmark'),
      localDir: '',
      remoteDir: '',
      syncBrowsing: false
    }
    mutate((draft) => findSiteNode(draft, siteId)?.bookmarks.push(bookmark))
    setSelection({ kind: 'bookmark', siteId, id: bookmark.id })
    setExpanded((s) => new Set(s).add(siteId))
  }

  const duplicate = () => {
    if (!selectedSite) {
      return
    }

    const source = selectedSite
    const copy: SiteConfig = {
      ...source.site,
      id: crypto.randomUUID(),
      name: `${source.site.name} (2)`
    }
    mutate((draft) => {
      let parent: SiteNode[] = draft.root
      walk(draft.root, (node, list) => {
        if (node.kind === 'site' && node.site.id === source.site.id) {
          parent = list

          return true
        }
      })
      parent.push({
        kind: 'site',
        site: copy,
        bookmarks: source.bookmarks.map((b) => ({ ...b, id: crypto.randomUUID() }))
      })
    })
    setSelection({ kind: 'site', id: copy.id })
  }

  const remove = () => {
    if (!selection) {
      return
    }

    const name =
      selection.kind === 'bookmark'
        ? selectedBookmark?.name
        : selection.kind === 'site'
          ? selectedSite?.site.name
          : findFolder(tree, selection.id)?.name
    useStore.getState().openDialog({
      kind: 'confirm',
      title: t('sm.delete'),
      message: t('sm.deleteConfirm', { name: name ?? '' }),
      danger: true,
      confirmLabel: t('sm.delete'),
      onConfirm: () => {
        mutate((draft) => {
          if (selection.kind === 'bookmark') {
            const node = findSiteNode(draft, selection.siteId)

            if (node) {
              node.bookmarks = node.bookmarks.filter((b) => b.id !== selection.id)
            }
          } else {
            detach(draft, selection.id)
          }
        })
        setSelection(null)
      }
    })
  }

  const rename = () => {
    if (!selection) {
      return
    }

    const current =
      selection.kind === 'bookmark'
        ? selectedBookmark?.name
        : selection.kind === 'site'
          ? selectedSite?.site.name
          : findFolder(tree, selection.id)?.name
    useStore.getState().openDialog({
      kind: 'input',
      title: t('sm.rename'),
      label: t('sm.name'),
      initial: current ?? '',
      onSubmit: (value) => {
        if (selection.kind === 'bookmark') {
          updateBookmark({ name: value })
        } else if (selection.kind === 'site') {
          updateSite({ name: value })
        } else {
          mutate((draft) => {
            const folder = findFolder(draft, selection.id)

            if (folder) {
              folder.name = value
            }
          })
        }
      }
    })
  }

  const moveNode = (nodeId: string, targetFolderId: string | null) => {
    mutate((draft) => {
      const moving = detach(draft, nodeId)

      if (!moving) {
        return
      }

      if (targetFolderId && containsFolder(moving, targetFolderId)) {
        draft.root.push(moving)

        return
      }

      const target = targetFolderId ? findFolder(draft, targetFolderId)?.children : draft.root
      ;(target ?? draft.root).push(moving)
    })

    if (targetFolderId) {
      setExpanded((s) => new Set(s).add(targetFolderId))
    }
  }

  const persist = async () => {
    for (const [siteId, password] of Object.entries(passwords)) {
      const node = findSiteNode(tree, siteId)

      if (node) {
        await api.sitePasswordSet(siteId, node.site.user, password).catch(toastError)
      }
    }
    await saveSites(tree)
  }

  const connect = async () => {
    if (!selectedSite) {
      return
    }

    await persist()
    close()
    const site = selectedSite.site
    const typed = passwords[site.id]
    const bookmark = selectedBookmark
      ? {
        localDir: selectedBookmark.localDir,
        remoteDir: selectedBookmark.remoteDir,
        syncBrowsing: selectedBookmark.syncBrowsing
      }
      : null
    void connectSite(site, {
      tabId: activeTabId,
      password: site.logonType === 'normal' ? (typed ?? null) : undefined,
      bookmark
    })
  }

  const startDrag = (event: React.MouseEvent, node: SiteNode) => {
    const nodeId = node.kind === 'folder' ? node.id : node.site.id
    const label = node.kind === 'folder' ? node.name : node.site.name || node.site.host
    startMouseDrag(event, {
      label,
      onMove: (target) => {
        const element = target?.closest<HTMLElement>('[data-node-id]')
        const folder =
          element?.dataset.nodeKind === 'folder' ? (element.dataset.nodeId ?? null) : null
        setDropTarget(target?.closest('.tree') ? (folder ?? '__root__') : null)
      },
      onDrop: (target) => {
        setDropTarget(null)

        if (!target?.closest('.tree')) {
          return
        }

        const element = target.closest<HTMLElement>('[data-node-id]')
        const folder =
          element?.dataset.nodeKind === 'folder' ? (element.dataset.nodeId ?? null) : null

        if (folder === nodeId) {
          return
        }

        moveNode(nodeId, folder)
      }
    })
  }

  const renderNodes = (nodes: SiteNode[], depth: number): React.ReactNode =>
    nodes.map((node) => {
      if (node.kind === 'folder') {
        const open = expanded.has(node.id)
        const selected = selection?.kind === 'folder' && selection.id === node.id

        return (
          <div key={node.id}>
            <div
              className={`node${selected ? ' selected' : ''}${dropTarget === node.id ? ' drop' : ''}`}
              style={{ paddingLeft: 6 + depth * 14 }}
              data-node-id={node.id}
              data-node-kind="folder"
              onClick={() => setSelection({ kind: 'folder', id: node.id })}
              onDoubleClick={() =>
                setExpanded((s) => {
                  const next = new Set(s)

                  if (next.has(node.id)) {
                    next.delete(node.id)
                  } else {
                    next.add(node.id)
                  }

                  return next
                })
              }
              onMouseDown={(e) => startDrag(e, node)}
            >
              <span
                className="toggle"
                onClick={(e) => {
                  e.stopPropagation()
                  setExpanded((s) => {
                    const next = new Set(s)

                    if (next.has(node.id)) {
                      next.delete(node.id)
                    } else {
                      next.add(node.id)
                    }

                    return next
                  })
                }}
              >
                {open ? '▼' : '▶'}
              </span>
              <span>📁 {node.name}</span>
            </div>
            {open && renderNodes(node.children, depth + 1)}
          </div>
        )
      }

      const selected = selection?.kind === 'site' && selection.id === node.site.id
      const open = expanded.has(node.site.id)

      return (
        <div key={node.site.id}>
          <div
            className={`node${selected ? ' selected' : ''}`}
            style={{ paddingLeft: 6 + depth * 14 }}
            data-node-id={node.site.id}
            data-node-kind="site"
            onClick={() => setSelection({ kind: 'site', id: node.site.id })}
            onDoubleClick={() => void connect()}
            onMouseDown={(e) => startDrag(e, node)}
          >
            <span
              className="toggle"
              onClick={(e) => {
                e.stopPropagation()

                if (node.bookmarks.length) {
                  setExpanded((s) => {
                    const next = new Set(s)

                    if (next.has(node.site.id)) {
                      next.delete(node.site.id)
                    } else {
                      next.add(node.site.id)
                    }

                    return next
                  })
                }
              }}
            >
              {node.bookmarks.length ? (open ? '▼' : '▶') : ''}
            </span>
            <span
              className="swatch"
              style={{
                background: node.site.color ?? 'transparent',
                border: node.site.color ? undefined : '1px solid var(--border)'
              }}
            />
            <span>{node.site.name || node.site.host}</span>
          </div>
          {open &&
            node.bookmarks.map((bookmark) => (
              <div
                key={bookmark.id}
                className={`node${selection?.kind === 'bookmark' && selection.id === bookmark.id ? ' selected' : ''}`}
                style={{ paddingLeft: 6 + (depth + 1) * 14 }}
                onClick={() =>
                  setSelection({ kind: 'bookmark', siteId: node.site.id, id: bookmark.id })
                }
                onDoubleClick={() => void connect()}
              >
                <span className="toggle" />
                <span>🔖 {bookmark.name}</span>
              </div>
            ))}
        </div>
      )
    })

  const site = selectedSite?.site ?? null
  const password = site ? (passwords[site.id] ?? '') : ''

  return (
    <Modal
      title={t('sm.title')}
      size="large"
      onClose={close}
      footer={
        <>
          <div className="left">
            <button onClick={() => void importFileZilla()}>{t('menu.importFileZilla')}</button>
            <button onClick={() => void importWinScp()}>{t('menu.importWinScp')}</button>
          </div>
          <button className="primary" disabled={!site} onClick={() => void connect()}>
            {t('sm.connect')}
          </button>
          <button onClick={() => void persist().then(close)}>{t('sm.ok')}</button>
          <button onClick={close}>{t('sm.cancel')}</button>
        </>
      }
    >
      <div className="sitemanager">
        <div className="left">
          <div
            className={`tree${dropTarget === '__root__' ? ' drop' : ''}`}
            onClick={(e) => {
              if (e.target === e.currentTarget) {
                setSelection(null)
              }
            }}
          >
            {renderNodes(tree.root, 0)}
          </div>
          <div className="tree-actions">
            <button onClick={addSite}>{t('sm.newSite')}</button>
            <button onClick={addFolder}>{t('sm.newFolder')}</button>
            <button onClick={addBookmark} disabled={!site}>
              {t('sm.newBookmark')}
            </button>
            <button onClick={rename} disabled={!selection}>
              {t('sm.rename')}
            </button>
            <button onClick={remove} disabled={!selection}>
              {t('sm.delete')}
            </button>
            <button onClick={duplicate} disabled={!site || selection?.kind !== 'site'}>
              {t('sm.duplicate')}
            </button>
          </div>
        </div>
        <div className="right">
          {selectedBookmark && site ? (
            <BookmarkForm bookmark={selectedBookmark} onChange={updateBookmark} />
          ) : site ? (
            <>
              <div className="subtabs">
                {(
                  [
                    'general',
                    'advanced',
                    'transfer',
                    'charset',
                    ...(site.protocol === 'sftp' ? ['sftp'] : [])
                  ] as SubTab[]
                ).map((tab) => (
                  <button
                    key={tab}
                    className={subTab === tab ? 'active' : ''}
                    onClick={() => setSubTab(tab)}
                  >
                    {t(`sm.tab.${tab}` as const)}
                  </button>
                ))}
              </div>
              <div className="subtab-body">
                {subTab === 'general' && (
                  <GeneralForm
                    site={site}
                    password={password}
                    hasStoredPassword={Boolean(storedPassword[site.id])}
                    onPassword={(value) => setPasswords((p) => ({ ...p, [site.id]: value }))}
                    onChange={updateSite}
                  />
                )}
                {subTab === 'advanced' && <AdvancedForm site={site} onChange={updateSite} />}
                {subTab === 'transfer' && <TransferForm site={site} onChange={updateSite} />}
                {subTab === 'charset' && <CharsetForm site={site} onChange={updateSite} />}
                {subTab === 'sftp' && <SftpForm site={site} onChange={updateSite} />}
              </div>
            </>
          ) : (
            <div className="placeholder">{t('sm.noSelection')}</div>
          )}
        </div>
      </div>
    </Modal>
  )
}

interface FormProps {
  site: SiteConfig,
  onChange: (patch: Partial<SiteConfig>) => void
}

function GeneralForm({
  site,
  password,
  hasStoredPassword,
  onPassword,
  onChange
}: FormProps & {
  password: string,
  hasStoredPassword: boolean,
  onPassword: (value: string) => void
}) {
  const t = useT()
  const protocols: { value: Protocol, label: string }[] = [
    { value: 'ftp', label: t('proto.ftp') },
    { value: 'ftps_explicit', label: t('proto.ftpsExplicit') },
    { value: 'ftps_implicit', label: t('proto.ftpsImplicit') },
    { value: 'sftp', label: t('proto.sftp') }
  ]

  const logonTypes: { value: LogonType, label: string }[] = [
    { value: 'anonymous', label: t('sm.logon.anonymous') },
    { value: 'normal', label: t('sm.logon.normal') },
    { value: 'ask', label: t('sm.logon.ask') },
    { value: 'interactive', label: t('sm.logon.interactive') },
    { value: 'key_file', label: t('sm.logon.keyFile') }
  ]

  return (
    <div className="form">
      <span className="k">{t('sm.name')}</span>
      <input type="text" value={site.name} onChange={(e) => onChange({ name: e.target.value })} />
      <span className="k">{t('sm.protocol')}</span>
      <select
        value={site.protocol}
        onChange={(e) => {
          const protocol = e.target.value as Protocol
          const wasDefault = site.port === defaultPort(site.protocol)
          onChange({ protocol, port: wasDefault ? defaultPort(protocol) : site.port })
        }}
      >
        {protocols.map((p) => (
          <option key={p.value} value={p.value}>
            {p.label}
          </option>
        ))}
      </select>
      <span className="k">{t('sm.host')}</span>
      <div className="inline">
        <input
          type="text"
          value={site.host}
          spellCheck={false}
          onChange={(e) => onChange({ host: e.target.value.trim() })}
        />
        <span className="k">{t('sm.port')}</span>
        <input
          type="number"
          min={1}
          max={65535}
          style={{ width: 90 }}
          value={site.port}
          onChange={(e) =>
            onChange({
              port: Number(e.target.value) || defaultPort(site.protocol)
            })
          }
        />
      </div>
      <span className="k">{t('sm.logonType')}</span>
      <select
        value={site.logonType}
        onChange={(e) => onChange({ logonType: e.target.value as LogonType })}
      >
        {logonTypes
          .filter((l) => site.protocol === 'sftp' || l.value !== 'key_file')
          .map((l) => (
            <option key={l.value} value={l.value}>
              {l.label}
            </option>
          ))}
      </select>
      <span className="k">{t('sm.user')}</span>
      <input
        type="text"
        value={site.user}
        spellCheck={false}
        disabled={site.logonType === 'anonymous'}
        onChange={(e) => onChange({ user: e.target.value })}
      />
      {site.logonType === 'normal' && (
        <>
          <span className="k">{t('sm.password')}</span>
          <div>
            <input
              type="password"
              value={password}
              placeholder={hasStoredPassword ? '••••••••' : ''}
              onChange={(e) => onPassword(e.target.value)}
              style={{ width: '100%' }}
            />
            <div className="hint">{t('sm.passwordStored')}</div>
          </div>
        </>
      )}
      <span className="k">{t('sm.color')}</span>
      <div className="swatches">
        <span
          className={`sw none${site.color ? '' : ' selected'}`}
          title={t('sm.color.none')}
          onClick={() => onChange({ color: null })}
        />
        {COLORS.map((color) => (
          <span
            key={color}
            className={`sw${site.color === color ? ' selected' : ''}`}
            style={{ background: color }}
            onClick={() => onChange({ color })}
          />
        ))}
      </div>
      <span className="k">{t('sm.comments')}</span>
      <textarea
        rows={3}
        value={site.comments}
        onChange={(e) => onChange({ comments: e.target.value })}
      />
    </div>
  )
}

function AdvancedForm({ site, onChange }: FormProps) {
  const t = useT()
  const serverTypes: { value: ServerType, label: string }[] = [
    { value: 'auto', label: t('sm.server.auto') },
    { value: 'unix', label: t('sm.server.unix') },
    { value: 'windows', label: t('sm.server.windows') }
  ]

  return (
    <div className="form">
      <span className="k">{t('sm.localDir')}</span>
      <div className="inline">
        <input
          type="text"
          value={site.localDir}
          onChange={(e) => onChange({ localDir: e.target.value })}
        />
        <button
          onClick={() =>
            void pickDirectory(site.localDir).then((dir) => dir && onChange({ localDir: dir }))
          }
        >
          {t('sm.browse')}
        </button>
      </div>
      <span className="k">{t('sm.remoteDir')}</span>
      <input
        type="text"
        value={site.remoteDir}
        spellCheck={false}
        onChange={(e) => onChange({ remoteDir: e.target.value })}
      />
      <span className="k" />
      <label>
        <input
          type="checkbox"
          checked={site.syncBrowsing}
          onChange={(e) => onChange({ syncBrowsing: e.target.checked })}
        />
        {t('sm.syncBrowsing')}
      </label>
      <span className="k">{t('sm.serverType')}</span>
      <select
        value={site.serverType}
        onChange={(e) => onChange({ serverType: e.target.value as ServerType })}
      >
        {serverTypes.map((s) => (
          <option key={s.value} value={s.value}>
            {s.label}
          </option>
        ))}
      </select>
      <span className="k" />
      <label>
        <input
          type="checkbox"
          checked={site.bypassProxy}
          onChange={(e) => onChange({ bypassProxy: e.target.checked })}
        />
        {t('sm.bypassProxy')}
      </label>
      <span className="k">{t('sm.keepalive')}</span>
      <input
        type="number"
        min={0}
        value={site.keepaliveSecs}
        onChange={(e) =>
          onChange({
            keepaliveSecs: Math.max(0, Number(e.target.value) || 0)
          })
        }
      />
      <span className="k">{t('sm.timeout')}</span>
      <input
        type="number"
        min={5}
        value={site.timeoutSecs}
        onChange={(e) =>
          onChange({
            timeoutSecs: Math.max(5, Number(e.target.value) || 30)
          })
        }
      />
    </div>
  )
}

function TransferForm({ site, onChange }: FormProps) {
  const t = useT()
  const modes: { value: TransferMode, label: string }[] = [
    { value: 'default', label: t('sm.mode.default') },
    { value: 'active', label: t('sm.mode.active') },
    { value: 'passive', label: t('sm.mode.passive') }
  ]

  return (
    <div className="form">
      {site.protocol !== 'sftp' && (
        <>
          <span className="k">{t('sm.transferMode')}</span>
          <select
            value={site.transferMode}
            onChange={(e) => onChange({ transferMode: e.target.value as TransferMode })}
          >
            {modes.map((m) => (
              <option key={m.value} value={m.value}>
                {m.label}
              </option>
            ))}
          </select>
        </>
      )}
      {site.protocol !== 'sftp' && (
        <>
          <span className="k" />
          <label>
            <input
              type="checkbox"
              checked={site.asciiMode}
              onChange={(e) => onChange({ asciiMode: e.target.checked })}
            />
            {t('sm.asciiMode')}
          </label>
        </>
      )}
      <span className="k">{t('sm.maxConnections')}</span>
      <input
        type="number"
        min={1}
        max={10}
        value={site.maxConnections}
        onChange={(e) =>
          onChange({
            maxConnections: Math.min(10, Math.max(1, Number(e.target.value) || 1))
          })
        }
      />
      <span className="k">{t('sm.conflictPolicy')}</span>
      <ConflictPolicySelect
        value={site.conflictPolicy}
        onChange={(value) => onChange({ conflictPolicy: value })}
        allowInherit
      />
    </div>
  )
}

function CharsetForm({ site, onChange }: FormProps) {
  const t = useT()
  const mode = site.encoding.mode

  return (
    <div className="form">
      <span className="k full">
        <label>
          <input
            type="radio"
            checked={mode === 'auto'}
            onChange={() => onChange({ encoding: { mode: 'auto' } })}
          />
          {t('sm.charset.auto')}
        </label>
      </span>
      <span className="k full">
        <label>
          <input
            type="radio"
            checked={mode === 'utf8'}
            onChange={() => onChange({ encoding: { mode: 'utf8' } })}
          />
          {t('sm.charset.utf8')}
        </label>
      </span>
      <span className="k full">
        <label>
          <input
            type="radio"
            checked={mode === 'custom'}
            onChange={() => onChange({ encoding: { mode: 'custom', label: 'ISO-8859-1' } })}
          />
          {t('sm.charset.custom')}
        </label>
      </span>
      {mode === 'custom' && (
        <>
          <span className="k">{t('sm.customEncoding')}</span>
          <input
            type="text"
            value={site.encoding.mode === 'custom' ? site.encoding.label : ''}
            onChange={(e) => onChange({ encoding: { mode: 'custom', label: e.target.value } })}
          />
        </>
      )}
    </div>
  )
}

function SftpForm({ site, onChange }: FormProps) {
  const t = useT()
  const sftp = site.sftp
  const patch = (value: Partial<SiteConfig['sftp']>) => onChange({ sftp: { ...sftp, ...value } })

  return (
    <div className="form">
      <span className="k">{t('sm.keyPath')}</span>
      <div className="inline">
        <input
          type="text"
          value={sftp.keyPath}
          spellCheck={false}
          onChange={(e) => patch({ keyPath: e.target.value })}
        />
        <button
          onClick={() =>
            void pickFile(sftp.keyPath).then((file) => file && patch({ keyPath: file }))
          }
        >
          {t('sm.browse')}
        </button>
      </div>
      <span className="k" />
      <span className="hint">{t('sm.keyHint')}</span>
      <span className="k" />
      <label>
        <input
          type="checkbox"
          checked={sftp.useAgent}
          onChange={(e) => patch({ useAgent: e.target.checked })}
        />
        {t('sm.useAgent')}
      </label>
      <span className="k">{t('sm.proxyJump')}</span>
      <input
        type="text"
        value={sftp.proxyJump}
        spellCheck={false}
        placeholder="user@bastion:22"
        onChange={(e) => patch({ proxyJump: e.target.value })}
      />
      <span className="k">{t('sm.extraOptions')}</span>
      <div>
        <textarea
          rows={4}
          value={sftp.extraOptions.join('\n')}
          spellCheck={false}
          onChange={(e) =>
            patch({
              extraOptions: e.target.value.split('\n')
            })
          }
          style={{ width: '100%', fontFamily: 'var(--mono)' }}
        />
        <div className="hint">{t('sm.extraOptionsHint')}</div>
      </div>
    </div>
  )
}

function BookmarkForm({
  bookmark,
  onChange
}: {
  bookmark: Bookmark,
  onChange: (patch: Partial<Bookmark>) => void
}) {
  const t = useT()

  return (
    <div className="form" style={{ padding: '6px 2px' }}>
      <span className="k">{t('sm.name')}</span>
      <input
        type="text"
        value={bookmark.name}
        onChange={(e) => onChange({ name: e.target.value })}
      />
      <span className="k">{t('sm.bookmarkLocal')}</span>
      <div className="inline">
        <input
          type="text"
          value={bookmark.localDir}
          onChange={(e) => onChange({ localDir: e.target.value })}
        />
        <button
          onClick={() =>
            void pickDirectory(bookmark.localDir).then((dir) => dir && onChange({ localDir: dir }))
          }
        >
          {t('sm.browse')}
        </button>
      </div>
      <span className="k">{t('sm.bookmarkRemote')}</span>
      <input
        type="text"
        value={bookmark.remoteDir}
        spellCheck={false}
        onChange={(e) => onChange({ remoteDir: e.target.value })}
      />
      <span className="k" />
      <label>
        <input
          type="checkbox"
          checked={bookmark.syncBrowsing}
          onChange={(e) => onChange({ syncBrowsing: e.target.checked })}
        />
        {t('sm.syncBrowsing')}
      </label>
    </div>
  )
}