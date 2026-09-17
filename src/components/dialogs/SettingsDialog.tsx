import { useState } from 'react'
import { Modal } from '@/components/Modal'
import { languages } from '@/i18n'
import type { AppSettings, ConflictPolicy, SymlinkDownload, Theme } from '@/lib/api'
import { pickDirectory, saveSettings } from '@/state/actions'
import { useStore, useT } from '@/state/store'

const POLICIES: ConflictPolicy[] = [
  'ask',
  'overwrite',
  'skip',
  'rename',
  'resume',
  'overwrite_if_newer'
]

export function policyLabel(t: ReturnType<typeof useT>, policy: ConflictPolicy): string {
  switch (policy) {
    case 'ask':
      return t('conflict.ask')
    case 'overwrite':
      return t('conflict.overwrite')
    case 'skip':
      return t('conflict.skip')
    case 'rename':
      return t('conflict.rename')
    case 'resume':
      return t('conflict.resume')
    case 'overwrite_if_newer':
      return t('conflict.overwriteIfNewer')
  }
}

export function ConflictPolicySelect({
  value,
  onChange,
  allowInherit
}: {
  value: ConflictPolicy | null,
  onChange: (value: ConflictPolicy | null) => void,
  allowInherit?: boolean
}) {
  const t = useT()

  return (
    <select
      value={value ?? ''}
      onChange={(e) => onChange(e.target.value ? (e.target.value as ConflictPolicy) : null)}
    >
      {allowInherit && <option value="">{t('sm.useGlobal')}</option>}
      {POLICIES.map((policy) => (
        <option key={policy} value={policy}>
          {policyLabel(t, policy)}
        </option>
      ))}
    </select>
  )
}

export function SettingsDialog({ close }: { close: () => void }) {
  const t = useT()
  const current = useStore((s) => s.settings)
  const [draft, setDraft] = useState<AppSettings>({ ...current })
  const update = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((d) => ({ ...d, [key]: value }))

  const number = (key: keyof AppSettings, value: string, min = 0) =>
    update(key, Math.max(min, Number(value) || 0) as never)

  return (
    <Modal
      title={t('settings.title')}
      size="wide"
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('dialog.cancel')}</button>
          <button
            className="primary"
            onClick={() => {
              close()
              void saveSettings(draft)
            }}
          >
            {t('settings.save')}
          </button>
        </>
      }
    >
      <div className="form">
        <span className="k full" style={{ fontWeight: 600 }}>
          {t('settings.general')}
        </span>
        <span className="k">{t('settings.theme')}</span>
        <select value={draft.theme} onChange={(e) => update('theme', e.target.value as Theme)}>
          <option value="dark">{t('menu.themeDark')}</option>
          <option value="light">{t('menu.themeLight')}</option>
          <option value="system">{t('menu.themeSystem')}</option>
        </select>
        <span className="k">{t('settings.language')}</span>
        <select value={draft.language} onChange={(e) => update('language', e.target.value)}>
          {languages.map((lang) => (
            <option key={lang.code} value={lang.code}>
              {lang.label}
            </option>
          ))}
        </select>
        <span className="k">{t('sm.localDir')}</span>
        <div className="inline">
          <input
            type="text"
            value={draft.defaultLocalDir}
            onChange={(e) => update('defaultLocalDir', e.target.value)}
          />
          <button
            onClick={() =>
              void pickDirectory(draft.defaultLocalDir).then(
                (dir) => dir && update('defaultLocalDir', dir)
              )
            }
          >
            {t('sm.browse')}
          </button>
        </div>
        <span className="k" />
        <label>
          <input
            type="checkbox"
            checked={draft.showHidden}
            onChange={(e) => update('showHidden', e.target.checked)}
          />
          {t('settings.showHidden')}
        </label>
        <span className="k">{t('settings.paneOrder')}</span>
        <select
          value={draft.localPaneLeft ? 'local_left' : 'remote_left'}
          onChange={(e) => update('localPaneLeft', e.target.value === 'local_left')}
        >
          <option value="local_left">{t('settings.paneOrder.localLeft')}</option>
          <option value="remote_left">{t('settings.paneOrder.remoteLeft')}</option>
        </select>
        <span className="k" />
        <label>
          <input
            type="checkbox"
            checked={draft.systemIcons}
            onChange={(e) => update('systemIcons', e.target.checked)}
          />
          {t('settings.systemIcons')}
        </label>
        <span className="k" />
        <label>
          <input
            type="checkbox"
            checked={draft.confirmDelete}
            onChange={(e) => update('confirmDelete', e.target.checked)}
          />
          {t('settings.confirmDelete')}
        </label>
        <span className="k">{t('settings.cacheTtl')}</span>
        <input
          type="number"
          min={1}
          value={draft.cacheTtlSecs}
          onChange={(e) => number('cacheTtlSecs', e.target.value, 1)}
        />

        <span className="k full" style={{ fontWeight: 600, marginTop: 8 }}>
          {t('settings.transfers')}
        </span>
        <span className="k">{t('settings.conflictPolicy')}</span>
        <ConflictPolicySelect
          value={draft.defaultConflictPolicy}
          onChange={(v) => update('defaultConflictPolicy', v ?? 'ask')}
        />
        <span className="k">{t('settings.symlinkDownload')}</span>
        <select
          value={draft.symlinkDownload}
          onChange={(e) => update('symlinkDownload', e.target.value as SymlinkDownload)}
        >
          <option value="follow">{t('settings.symlink.follow')}</option>
          <option value="copy_link">{t('settings.symlink.copyLink')}</option>
        </select>
        <span className="k">{t('settings.maxRetries')}</span>
        <input
          type="number"
          min={0}
          value={draft.maxRetries}
          onChange={(e) => number('maxRetries', e.target.value)}
        />
        <span className="k">{t('settings.retryBackoff')}</span>
        <input
          type="number"
          min={250}
          step={250}
          value={draft.retryBackoffMs}
          onChange={(e) => number('retryBackoffMs', e.target.value, 250)}
        />
        <span className="k">{t('settings.bandwidth')}</span>
        <input
          type="number"
          min={0}
          value={draft.bandwidthLimitKbps}
          onChange={(e) => number('bandwidthLimitKbps', e.target.value)}
        />
        <span className="k" />
        <label>
          <input
            type="checkbox"
            checked={draft.verifyHash}
            onChange={(e) => update('verifyHash', e.target.checked)}
          />
          {t('settings.verifyHash')}
        </label>

        <span className="k full" style={{ fontWeight: 600, marginTop: 8 }}>
          {t('settings.connection')}
        </span>
        <span className="k">{t('settings.keepalive')}</span>
        <input
          type="number"
          min={0}
          value={draft.keepaliveSecs}
          onChange={(e) => number('keepaliveSecs', e.target.value)}
        />
        <span className="k">{t('settings.timeout')}</span>
        <input
          type="number"
          min={5}
          value={draft.timeoutSecs}
          onChange={(e) => number('timeoutSecs', e.target.value, 5)}
        />
      </div>
    </Modal>
  )
}