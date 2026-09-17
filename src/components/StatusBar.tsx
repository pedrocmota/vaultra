import { totalSize } from '@/lib/entries'
import { formatBytes, formatSpeed } from '@/lib/format'
import { useStore, useT } from '@/state/store'

export function StatusBar() {
  const t = useT()
  const stats = useStore((s) => s.stats)
  const tab = useStore((s) => s.tabs.find((x) => x.id === s.activeTabId))
  const focusedPane = useStore((s) => s.focusedPane)
  const pane = tab?.[focusedPane]
  const selectedSet = new Set(pane?.selected ?? [])
  const selectedEntries = pane ? pane.entries.filter((e) => selectedSet.has(e.path)) : []

  return (
    <div className="statusbar">
      <span>
        {selectedEntries.length > 0
          ? t('status.selected', {
            n: selectedEntries.length,
            size: formatBytes(totalSize(selectedEntries))
          })
          : t('status.items', { n: pane?.entries.length ?? 0 })}
      </span>
      <span className="spacer" />
      <span className={stats.failed > 0 ? 'warn' : ''}>
        {t('status.queue', { active: stats.active, queued: stats.queued, failed: stats.failed })}
      </span>
      <span>{stats.speedBps > 0 ? formatSpeed(stats.speedBps) : t('status.idle')}</span>
    </div>
  )
}