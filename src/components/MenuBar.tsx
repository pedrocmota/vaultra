import {useEffect, useRef, useState} from 'react'
import {getCurrentWindow} from '@tauri-apps/api/window'
import {globalKeymap, paneKeymap} from '@/keybindings'
import {api, type RecentConnection} from '@/lib/api'
import {
  clearQueue,
  closeTab,
  connectSite,
  disconnectTab,
  importFileZilla,
  importWinScp,
  newTab,
  queueActions,
  refresh,
  updateSettings
} from '@/state/actions'
import {useStore, useT} from '@/state/store'
import type {MenuItem} from './ContextMenu'

export function MenuBar() {
  const t = useT()
  const settings = useStore((s) => s.settings)
  const system = useStore((s) => s.system)
  const activeTabId = useStore((s) => s.activeTabId)
  const tab = useStore((s) => s.tabs.find((x) => x.id === s.activeTabId))
  const focusedPane = useStore((s) => s.focusedPane)
  const queueLength = useStore(
    (s) => s.queue.items.filter((i) => i.status !== 'failed').length
  )

  const openDialog = useStore((s) => s.openDialog)
  const [open, setOpen] = useState<string | null>(null)
  const [recent, setRecent] = useState<RecentConnection[]>([])
  const barRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (open === 'file') {
      api
        .recentGet()
        .then(setRecent)
        .catch(() => setRecent([]))
    }
  }, [open])

  useEffect(() => {
    if (!open) {
      return
    }

    const close = (event: MouseEvent) => {
      if (barRef.current && !barRef.current.contains(event.target as Node)) {
        setOpen(null)
      }
    }

    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        setOpen(null)
      }
    }
    window.addEventListener('mousedown', close)
    window.addEventListener('keydown', key)

    return () => {
      window.removeEventListener('mousedown', close)
      window.removeEventListener('keydown', key)
    }
  }, [open])

  const menus: {id: string, label: string, items: MenuItem[]}[] = [
    {
      id: 'file',
      label: t('menu.file'),
      items: [
        {
          label: t('menu.siteManager'),
          shortcut: globalKeymap.label('siteManager'),
          onClick: () => openDialog({kind: 'siteManager'})
        },
        {
          label: t('menu.newTab'),
          shortcut: globalKeymap.label('newTab'),
          onClick: () => void newTab()
        },
        {
          label: t('menu.closeTab'),
          shortcut: globalKeymap.label('closeTab'),
          onClick: () => activeTabId && void closeTab(activeTabId)
        },
        {
          label: t('menu.disconnect'),
          disabled: !tab?.sessionId,
          onClick: () => activeTabId && void disconnectTab(activeTabId)
        },
        {separator: true},
        ...(recent.length
          ? [
            ...recent.slice(0, 8).map<MenuItem>((r) => ({
              label: `${r.site.name || r.site.host} (${r.site.user ? `${r.site.user}@` : ''}${r.site.host})`,
              onClick: () => void connectSite(r.site)
            })),
            {
              label: t('menu.clearRecent'),
              onClick: () => void api.recentClear().then(() => setRecent([]))
            },
            {separator: true}
          ]
          : []),
        {label: t('menu.importFileZilla'), onClick: () => void importFileZilla()},
        {label: t('menu.importWinScp'), onClick: () => void importWinScp()},
        {separator: true},
        {
          label: t('menu.exit'),
          shortcut: 'Alt+F4',
          onClick: () => void getCurrentWindow().close()
        }
      ]
    },
    {
      id: 'edit',
      label: t('menu.edit'),
      items: [{label: t('menu.settings'), onClick: () => openDialog({kind: 'settings'})}]
    },
    {
      id: 'view',
      label: t('menu.view'),
      items: [
        {
          label: t('menu.refresh'),
          shortcut: paneKeymap.label('refresh'),
          onClick: () => activeTabId && void refresh(activeTabId, focusedPane)
        },
        {
          label: t('menu.showHidden'),
          checked: settings.showHidden,
          onClick: () => void updateSettings({showHidden: !settings.showHidden})
        },
        {
          label: t('menu.swapPanes'),
          onClick: () => void updateSettings({localPaneLeft: !settings.localPaneLeft})
        },
        {separator: true},
        {
          label: `${t('menu.theme')}: ${t('menu.themeDark')}`,
          checked: settings.theme === 'dark',
          onClick: () => void updateSettings({theme: 'dark'})
        },
        {
          label: `${t('menu.theme')}: ${t('menu.themeLight')}`,
          checked: settings.theme === 'light',
          onClick: () => void updateSettings({theme: 'light'})
        },
        {
          label: `${t('menu.theme')}: ${t('menu.themeSystem')}`,
          checked: settings.theme === 'system',
          onClick: () => void updateSettings({theme: 'system'})
        }
      ]
    },
    {
      id: 'transfer',
      label: t('menu.transfer'),
      items: [
        {label: t('menu.processQueue'), onClick: () => void queueActions.resumeAll()},
        {label: t('menu.pauseAll'), onClick: () => void queueActions.pauseAll()},
        {label: t('menu.clearQueue'), onClick: () => clearQueue(queueLength)},
        {separator: true},
        {
          label: t('menu.sync'),
          disabled: !tab?.sessionId,
          onClick: () => activeTabId && openDialog({kind: 'sync', tabId: activeTabId})
        }
      ]
    },
    {
      id: 'help',
      label: t('menu.help'),
      items: [
        {label: t('menu.openLogs'), onClick: () => system && void api.openPath(system.logDir)},
        {label: t('menu.about'), onClick: () => openDialog({kind: 'about'})}
      ]
    }
  ]

  return (
    <div className="menubar" ref={barRef}>
      {menus.map((menu) => (
        <div key={menu.id} className={`menu${open === menu.id ? ' open' : ''}`}>
          <button
            onClick={() => setOpen(open === menu.id ? null : menu.id)}
            onMouseEnter={() => open && setOpen(menu.id)}
          >
            {menu.label}
          </button>
          {open === menu.id && (
            <div className="dropdown">
              {menu.items.map((item, index) =>
                item.separator ? (
                  <div key={index} className="sep" />
                ) : (
                  <div
                    key={index}
                    className={`item${item.disabled ? ' disabled' : ''}`}
                    onClick={() => {
                      if (item.disabled) {
                        return
                      }

                      setOpen(null)
                      item.onClick?.()
                    }}
                  >
                    <span>
                      {item.checked !== undefined && (
                        <span className="check">{item.checked ? '✓' : ''}</span>
                      )}
                      {item.label}
                    </span>
                    {item.shortcut && <span className="shortcut">{item.shortcut}</span>}
                  </div>
                )
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  )
}