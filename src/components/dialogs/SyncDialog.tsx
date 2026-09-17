import {useMemo, useState} from 'react'
import {Modal} from '@/components/Modal'
import {api, type DiffEntry, type DiffKind, type QueueRequest} from '@/lib/api'
import {formatBytes, formatDate} from '@/lib/format'
import {pickDirectory, toastError} from '@/state/actions'
import {useStore, useT} from '@/state/store'

type SyncAction = 'upload' | 'download' | 'none'

function defaultAction(kind: DiffKind): SyncAction {
  switch (kind) {
    case 'local_newer':
    case 'local_only':
      return 'upload'
    case 'remote_newer':
    case 'remote_only':
      return 'download'
    default:
      return 'none'
  }
}

export function SyncDialog({tabId, close}: {tabId: string, close: () => void}) {
  const t = useT()
  const language = useStore((s) => s.language)
  const tab = useStore((s) => s.tabs.find((x) => x.id === tabId))
  const [localDir, setLocalDir] = useState(tab?.local.path ?? '')
  const [remoteDir, setRemoteDir] = useState(tab?.remote.path ?? '')
  const [excludes, setExcludes] = useState('.git\nnode_modules\n*.tmp')
  const [diff, setDiff] = useState<DiffEntry[] | null>(null)
  const [actions, setActions] = useState<Record<string, SyncAction>>({})
  const [checked, setChecked] = useState<Record<string, boolean>>({})
  const [busy, setBusy] = useState(false)

  const differences = useMemo(() => (diff ?? []).filter((d) => d.kind !== 'equal'), [diff])

  const compare = async () => {
    if (!tab?.sessionId) {
      return
    }

    setBusy(true)

    try {
      const result = await api.syncCompare({
        sessionId: tab.sessionId,
        localDir,
        remoteDir,
        excludes: excludes
          .split('\n')
          .map((s) => s.trim())
          .filter(Boolean)
      })
      setDiff(result)
      const nextActions: Record<string, SyncAction> = {}
      const nextChecked: Record<string, boolean> = {}
      for (const entry of result) {
        if (entry.kind === 'equal') {
          continue
        }

        nextActions[entry.relativePath] = defaultAction(entry.kind)
        nextChecked[entry.relativePath] = nextActions[entry.relativePath] !== 'none'
      }
      setActions(nextActions)
      setChecked(nextChecked)
    } catch (error) {
      toastError(error)
    } finally {
      setBusy(false)
    }
  }

  const apply = async () => {
    if (!tab?.sessionId) {
      return
    }

    const requests: QueueRequest[] = []
    for (const entry of differences) {
      const action = actions[entry.relativePath]

      if (!checked[entry.relativePath] || action === 'none') {
        continue
      }

      requests.push({
        sessionId: tab.sessionId,
        direction: action,
        localPath: entry.localPath,
        remotePath: entry.remotePath,
        isDir: entry.isDir,
        size: entry.isDir ? null : action === 'upload' ? entry.localSize : entry.remoteSize,
        mtime: action === 'upload' ? entry.localMtime : entry.remoteMtime,
        conflictPolicy: 'overwrite'
      })
    }

    if (requests.length) {
      await api.queueAdd(requests).catch(toastError)
      useStore.getState().setBottomTab('queue')
    }

    close()
  }

  const kindLabel = (kind: DiffKind) => {
    switch (kind) {
      case 'local_newer':
        return t('sync.localNewer')
      case 'remote_newer':
        return t('sync.remoteNewer')
      case 'local_only':
        return t('sync.localOnly')
      case 'remote_only':
        return t('sync.remoteOnly')
      case 'different':
        return t('sync.different')
      default:
        return t('sync.equal')
    }
  }

  const allChecked = differences.length > 0 && differences.every((d) => checked[d.relativePath])

  return (
    <Modal
      title={t('sync.title')}
      size="large"
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('dialog.cancel')}</button>
          <button onClick={() => void compare()} disabled={busy || !tab?.sessionId}>
            {busy ? t('sync.comparing') : t('sync.compare')}
          </button>
          <button
            className="primary"
            onClick={() => void apply()}
            disabled={!diff || differences.length === 0}
          >
            {t('sync.apply')}
          </button>
        </>
      }
    >
      <div className="form">
        <span className="k">{t('sync.local')}</span>
        <div className="inline">
          <input type="text" value={localDir} onChange={(e) => setLocalDir(e.target.value)} />
          <button
            onClick={() => void pickDirectory(localDir).then((dir) => dir && setLocalDir(dir))}
          >
            {t('sm.browse')}
          </button>
        </div>
        <span className="k">{t('sync.remote')}</span>
        <input type="text" value={remoteDir} onChange={(e) => setRemoteDir(e.target.value)} />
        <span className="k">{t('sync.excludes')}</span>
        <textarea rows={3} value={excludes} onChange={(e) => setExcludes(e.target.value)} />
      </div>
      {diff && differences.length === 0 && <p className="hint">{t('sync.noDiff')}</p>}
      {differences.length > 0 && (
        <table className="table">
          <thead>
            <tr>
              <th>
                <input
                  type="checkbox"
                  checked={allChecked}
                  onChange={(e) => {
                    const value = e.target.checked
                    setChecked(
                      Object.fromEntries(
                        differences.map((d) => [
                          d.relativePath,
                          value && actions[d.relativePath] !== 'none'
                        ])
                      )
                    )
                  }}
                  title={t('sync.selectAll')}
                />
              </th>
              <th>{t('sync.col.path')}</th>
              <th>{t('sync.col.status')}</th>
              <th>{t('sync.col.local')}</th>
              <th>{t('sync.col.remote')}</th>
              <th>{t('sync.col.action')}</th>
            </tr>
          </thead>
          <tbody>
            {differences.map((entry) => (
              <tr key={entry.relativePath}>
                <td>
                  <input
                    type="checkbox"
                    checked={Boolean(checked[entry.relativePath])}
                    onChange={(e) =>
                      setChecked({...checked, [entry.relativePath]: e.target.checked})
                    }
                  />
                </td>
                <td title={`${entry.localPath}\n${entry.remotePath}`}>
                  {entry.isDir ? '📁 ' : ''}
                  {entry.relativePath}
                </td>
                <td>{kindLabel(entry.kind)}</td>
                <td>
                  {entry.localSize !== null
                    ? `${entry.isDir ? '' : formatBytes(entry.localSize)} ${formatDate(
                      entry.localMtime,
                      language
                    )}`
                    : '-'}
                </td>
                <td>
                  {entry.remoteSize !== null
                    ? `${entry.isDir ? '' : formatBytes(entry.remoteSize)} ${formatDate(
                      entry.remoteMtime,
                      language
                    )}`
                    : '-'}
                </td>
                <td className="diff-actions">
                  <select
                    value={actions[entry.relativePath] ?? 'none'}
                    onChange={(e) => {
                      const value = e.target.value as SyncAction
                      setActions({...actions, [entry.relativePath]: value})
                      setChecked({...checked, [entry.relativePath]: value !== 'none'})
                    }}
                  >
                    <option value="upload">{t('sync.action.upload')}</option>
                    <option value="download">{t('sync.action.download')}</option>
                    <option value="none">{t('sync.action.none')}</option>
                  </select>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Modal>
  )
}