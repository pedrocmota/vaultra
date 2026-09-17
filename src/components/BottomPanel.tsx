import { useEffect, useMemo, useRef, useState } from 'react'
import type { TransferItem } from '@/lib/api'
import {
  basename,
  formatBytes,
  formatDateTimeMs,
  formatSpeed,
  formatTime,
  percent
} from '@/lib/format'
import { queueActions } from '@/state/actions'
import { useStore, useT, type BottomTab } from '@/state/store'
import { ContextMenu, type MenuState } from './ContextMenu'

export function BottomPanel() {
  const t = useT()
  const bottomTab = useStore((s) => s.bottomTab)
  const setBottomTab = useStore((s) => s.setBottomTab)
  const queue = useStore((s) => s.queue)
  const bottomHeight = useStore((s) => s.bottomHeight)
  const failedCount = queue.items.filter((i) => i.status === 'failed').length
  const pendingCount = queue.items.filter((i) => i.status !== 'failed').length

  const tabs: { id: BottomTab, label: string, badge?: number, danger?: boolean }[] = [
    { id: 'queue', label: t('bottom.queue'), badge: pendingCount },
    { id: 'failed', label: t('bottom.failed'), badge: failedCount, danger: failedCount > 0 },
    { id: 'successful', label: t('bottom.successful'), badge: queue.history.length },
    { id: 'messages', label: t('bottom.messages') }
  ]

  return (
    <div className="bottom" style={{ height: bottomHeight }}>
      <div className="tabs">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            className={bottomTab === tab.id ? 'active' : ''}
            onClick={() => setBottomTab(tab.id)}
          >
            {tab.label}
            {tab.badge !== undefined && tab.badge > 0 && (
              <span className={`badge${tab.danger ? ' danger' : ''}`}>{tab.badge}</span>
            )}
          </button>
        ))}
        <div className="actions">
          {bottomTab === 'queue' && (
            <>
              <button onClick={() => void queueActions.resumeAll()}>{t('queue.resumeAll')}</button>
              <button onClick={() => void queueActions.pauseAll()}>{t('queue.pauseAll')}</button>
            </>
          )}
          {bottomTab === 'failed' && (
            <>
              <button onClick={() => void queueActions.retryFailed()}>{t('queue.retryAll')}</button>
              <button onClick={() => void queueActions.removeFailed()}>
                {t('queue.removeFailed')}
              </button>
            </>
          )}
          {bottomTab === 'successful' && (
            <button onClick={() => void queueActions.clearHistory()}>{t('queue.clear')}</button>
          )}
          {bottomTab === 'messages' && <MessagesActions />}
        </div>
      </div>
      <div className="content">
        {bottomTab === 'queue' && (
          <QueueTable
            items={queue.items.filter((i) => i.status !== 'failed')}
            emptyText={t('queue.empty')}
          />
        )}
        {bottomTab === 'failed' && (
          <QueueTable
            items={queue.items.filter((i) => i.status === 'failed')}
            emptyText={t('queue.noFailed')}
            showError
          />
        )}
        {bottomTab === 'successful' && <HistoryTable items={queue.history} />}
        {bottomTab === 'messages' && <MessagesView />}
      </div>
    </div>
  )
}

function statusLabel(t: ReturnType<typeof useT>, item: TransferItem): string {
  if (!item.sessionId && item.status !== 'failed') {
    return `${t(`status.${item.status}` as const)} (${t('queue.waitingConnection')})`
  }

  return t(`status.${item.status}` as const)
}

function QueueTable({
  items,
  emptyText,
  showError
}: {
  items: TransferItem[],
  emptyText: string,
  showError?: boolean
}) {
  const t = useT()
  const [menu, setMenu] = useState<MenuState | null>(null)
  const [selected, setSelected] = useState<string | null>(null)

  if (items.length === 0) {
    return (
      <div className="log" style={{ color: 'var(--text-muted)' }}>
        {emptyText}
      </div>
    )
  }

  const openMenu = (event: React.MouseEvent, item: TransferItem, index: number) => {
    event.preventDefault()
    setSelected(item.id)
    const paused = item.status === 'paused' || item.status === 'failed'
    setMenu({
      x: event.clientX,
      y: event.clientY,
      items: [
        {
          label: paused ? t('queue.resume') : t('queue.pause'),
          onClick: () => void (paused ? queueActions.resume(item.id) : queueActions.pause(item.id))
        },
        {
          label: t('queue.remove'),
          danger: true,
          onClick: () => void queueActions.remove(item.id)
        },
        { separator: true },
        {
          label: t('queue.moveTop'),
          disabled: index === 0,
          onClick: () => void queueActions.move(item.id, 0)
        },
        {
          label: t('queue.moveUp'),
          disabled: index === 0,
          onClick: () => void queueActions.move(item.id, Math.max(0, index - 1))
        },
        {
          label: t('queue.moveDown'),
          disabled: index >= items.length - 1,
          onClick: () => void queueActions.move(item.id, index + 1)
        },
        { separator: true },
        {
          label: `${t('queue.priority')}: ${t('queue.priorityHigh')}`,
          checked: item.priority > 0,
          onClick: () => void queueActions.priority(item.id, 10)
        },
        {
          label: `${t('queue.priority')}: ${t('queue.priorityNormal')}`,
          checked: item.priority === 0,
          onClick: () => void queueActions.priority(item.id, 0)
        },
        {
          label: `${t('queue.priority')}: ${t('queue.priorityLow')}`,
          checked: item.priority < 0,
          onClick: () => void queueActions.priority(item.id, -10)
        }
      ]
    })
  }

  return (
    <>
      <table className="table">
        <thead>
          <tr>
            <th>{t('queue.col.file')}</th>
            <th>{t('queue.col.direction')}</th>
            <th>{t('queue.col.site')}</th>
            <th className="num">{t('queue.col.size')}</th>
            <th>{t('queue.col.progress')}</th>
            <th className="num">{t('queue.col.speed')}</th>
            <th>{t('queue.col.status')}</th>
            {showError && <th>{t('queue.col.error')}</th>}
          </tr>
        </thead>
        <tbody>
          {items.map((item, index) => {
            const pct = percent(item.transferred, item.size)

            return (
              <tr
                key={item.id}
                className={selected === item.id ? 'selected' : ''}
                onClick={() => setSelected(item.id)}
                onContextMenu={(e) => openMenu(e, item, index)}
                title={`${item.localPath}\n${item.remotePath}`}
              >
                <td>
                  {item.isDir ? '📁 ' : ''}
                  {basename(item.direction === 'download' ? item.remotePath : item.localPath)}
                </td>
                <td>
                  {item.direction === 'download'
                    ? '↓ ' + t('dir.download')
                    : '↑ ' + t('dir.upload')}
                </td>
                <td>{item.siteName}</td>
                <td className="num">{item.isDir ? '' : formatBytes(item.size)}</td>
                <td>
                  {!item.isDir && (
                    <div className={`progress ${item.status}`}>
                      <div className="bar" style={{ width: `${pct}%` }} />
                      <div className="pct">{pct}%</div>
                    </div>
                  )}
                </td>
                <td className="num">
                  {item.status === 'active' && item.speedBps > 0 ? formatSpeed(item.speedBps) : ''}
                </td>
                <td>{statusLabel(t, item)}</td>
                {showError && <td className="err">{item.error}</td>}
              </tr>
            )
          })}
        </tbody>
      </table>
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />
      )}
    </>
  )
}

function HistoryTable({ items }: { items: TransferItem[] }) {
  const t = useT()
  const language = useStore((s) => s.language)

  if (items.length === 0) {
    return (
      <div className="log" style={{ color: 'var(--text-muted)' }}>
        {t('queue.noHistory')}
      </div>
    )
  }

  return (
    <table className="table">
      <thead>
        <tr>
          <th>{t('queue.col.file')}</th>
          <th>{t('queue.col.direction')}</th>
          <th>{t('queue.col.site')}</th>
          <th className="num">{t('queue.col.size')}</th>
          <th>{t('queue.col.status')}</th>
          <th>{t('queue.col.finished')}</th>
        </tr>
      </thead>
      <tbody>
        {items.map((item) => (
          <tr key={item.id} title={`${item.localPath}\n${item.remotePath}`}>
            <td>{basename(item.direction === 'download' ? item.remotePath : item.localPath)}</td>
            <td>{item.direction === 'download' ? t('dir.download') : t('dir.upload')}</td>
            <td>{item.siteName}</td>
            <td className="num">{item.isDir ? '' : formatBytes(item.size)}</td>
            <td>{t(`status.${item.status}` as const)}</td>
            <td>{formatDateTimeMs(item.finishedAt, language)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function MessagesActions() {
  const t = useT()
  const showTrace = useStore((s) => s.showTrace)
  const setShowTrace = useStore((s) => s.setShowTrace)
  const clearLogs = useStore((s) => s.clearLogs)

  return (
    <>
      <label style={{ fontSize: 12 }}>
        <input
          type="checkbox"
          checked={showTrace}
          onChange={(e) => setShowTrace(e.target.checked)}
        />
        {t('log.showTrace')}
      </label>
      <button onClick={clearLogs}>{t('log.clear')}</button>
    </>
  )
}

function MessagesView() {
  const t = useT()
  const logs = useStore((s) => s.logs)
  const showTrace = useStore((s) => s.showTrace)
  const language = useStore((s) => s.language)
  const activeSession = useStore(
    (s) => s.tabs.find((x) => x.id === s.activeTabId)?.sessionId ?? null
  )

  const ref = useRef<HTMLDivElement>(null)
  const [autoscroll, setAutoscroll] = useState(true)

  const visible = useMemo(
    () =>
      logs
        .filter(
          (l) =>
            (showTrace || l.kind !== 'trace') &&
            (!l.sessionId || !activeSession || l.sessionId === activeSession)
        )
        .slice(-1500),
    [logs, showTrace, activeSession]
  )

  useEffect(() => {
    if (autoscroll && ref.current) {
      ref.current.scrollTop = ref.current.scrollHeight
    }
  }, [visible, autoscroll])

  return (
    <div
      ref={ref}
      className="log"
      style={{ height: '100%', overflow: 'auto' }}
      onScroll={(e) => {
        const el = e.currentTarget
        setAutoscroll(el.scrollHeight - el.scrollTop - el.clientHeight < 8)
      }}
    >
      {visible.map((line, index) => (
        <div key={index} className={`line ${line.kind}`}>
          <span className="k" title={formatTime(line.ts, language)}>
            {t(`log.${line.kind}` as const)}:
          </span>
          <span className="t">{line.text}</span>
        </div>
      ))}
    </div>
  )
}