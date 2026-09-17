import {useEffect, useRef} from 'react'
import {getCurrentWebview} from '@tauri-apps/api/webview'
import {BottomPanel} from './components/BottomPanel'
import {Dialogs} from './components/dialogs/Dialogs'
import {MenuBar} from './components/MenuBar'
import {Pane} from './components/Pane'
import {QuickConnectBar} from './components/QuickConnectBar'
import {StatusBar} from './components/StatusBar'
import {TabBar} from './components/TabBar'
import {Toasts} from './components/Toasts'
import {useBrowserShortcutGuard, useGlobalKeymap} from './hooks/useKeymap'
import {browserShortcuts, globalKeymap} from './keybindings'
import {bootstrap, closeTab, newTab, refresh, toastError, uploadLocalPaths} from './state/actions'
import {useStore, useT} from './state/store'

export function App() {
  const t = useT()
  const tabs = useStore((s) => s.tabs)
  const activeTabId = useStore((s) => s.activeTabId)
  const settings = useStore((s) => s.settings)
  const system = useStore((s) => s.system)
  const openDialog = useStore((s) => s.openDialog)
  const setActiveTab = useStore((s) => s.setActiveTab)
  const focusedPane = useStore((s) => s.focusedPane)
  const dialogsOpen = useStore((s) => s.dialogs.length > 0)
  const setBottomHeight = useStore((s) => s.setBottomHeight)
  const bottomHeight = useStore((s) => s.bottomHeight)
  const paneSplit = useStore((s) => s.paneSplit)
  const setPaneSplit = useStore((s) => s.setPaneSplit)
  const panesRef = useRef<HTMLDivElement>(null)
  const workspaceRef = useRef<HTMLDivElement>(null)
  const highlighted = useRef<Element | null>(null)

  useEffect(() => {
    bootstrap().catch(toastError)
  }, [])

  const cycleTab = (offset: number) => {
    if (tabs.length === 0) {
      return
    }

    const index = tabs.findIndex((tab) => tab.id === activeTabId)
    const next = tabs[(index + offset + tabs.length) % tabs.length]
    setActiveTab(next.id)
  }

  useGlobalKeymap(
    globalKeymap,
    {
      newTab: () => newTab(),
      closeTab: () => activeTabId && closeTab(activeTabId),
      siteManager: () => openDialog({kind: 'siteManager'}),
      nextTab: () => cycleTab(1),
      previousTab: () => cycleTab(-1),
      refresh: () => activeTabId && refresh(activeTabId, focusedPane)
    },
    !dialogsOpen
  )
  useBrowserShortcutGuard(browserShortcuts)

  useEffect(() => {
    let unlisten: (() => void) | null = null
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const clear = () => {
          highlighted.current?.classList.remove('drop-target')
          highlighted.current = null
        }

        if (event.payload.type === 'leave') {
          clear()

          return
        }

        const position = event.payload.position
        const scale = window.devicePixelRatio || 1
        const element = document.elementFromPoint(position.x / scale, position.y / scale)
        const pane = element?.closest<HTMLElement>('.pane[data-side=\'remote\']')
        const list = pane?.querySelector('.filelist') ?? null

        if (event.payload.type === 'enter' || event.payload.type === 'over') {
          if (list !== highlighted.current) {
            clear()

            if (list) {
              list.classList.add('drop-target')
              highlighted.current = list
            }
          }

          return
        }

        if (event.payload.type === 'drop') {
          clear()

          if (!pane) {
            return
          }

          const tabId = pane.dataset.tab
          const row = element?.closest<HTMLElement>('.row[data-path]')
          const destDir = row && row.dataset.dir === '1' ? row.dataset.path : undefined

          if (tabId) {
            void uploadLocalPaths(tabId, event.payload.paths, destDir)
          }
        }
      })
      .then((fn) => {
        unlisten = fn
      })
      .catch(() => undefined)

    return () => {
      unlisten?.()
    }
  }, [])

  const startSplit = (event: React.MouseEvent) => {
    event.preventDefault()
    const startY = event.clientY
    const startHeight = bottomHeight
    const move = (e: MouseEvent) => setBottomHeight(startHeight - (e.clientY - startY))
    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }

  const startPaneSplit = (event: React.MouseEvent) => {
    event.preventDefault()
    const move = (e: MouseEvent) => {
      const rect = panesRef.current?.getBoundingClientRect()

      if (rect && rect.width > 0) {
        setPaneSplit((e.clientX - rect.left) / rect.width)
      }
    }

    const up = () => {
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', up)
    }
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', up)
  }

  const activeTab = tabs.find((tab) => tab.id === activeTabId)
  const paneOrder: ('local' | 'remote')[] = settings.localPaneLeft
    ? ['local', 'remote']
    : ['remote', 'local']

  return (
    <div className="app">
      <MenuBar />
      {system && !system.openssh && <div className="banner">⚠ {t('app.openSshMissing')}</div>}
      <QuickConnectBar />
      <TabBar />
      <div className="workspace" ref={workspaceRef}>
        <div className="panes" ref={panesRef}>
          {activeTab && (
            <>
              <Pane
                key={`${activeTab.id}-${paneOrder[0]}`}
                tabId={activeTab.id}
                side={paneOrder[0]}
                style={{flex: `${paneSplit} 1 0%`}}
              />
              <div className="vsplitter" onMouseDown={startPaneSplit} />
              <Pane
                key={`${activeTab.id}-${paneOrder[1]}`}
                tabId={activeTab.id}
                side={paneOrder[1]}
                style={{flex: `${1 - paneSplit} 1 0%`}}
              />
            </>
          )}
        </div>
        <div className="splitter" onMouseDown={startSplit} />
        <BottomPanel />
      </div>
      <StatusBar />
      <Dialogs />
      <Toasts />
    </div>
  )
}