import {useState} from 'react'
import {Modal} from '@/components/Modal'
import type {Entry} from '@/lib/entries'
import {fromOctal, toOctal} from '@/lib/format'
import {applyPermissions} from '@/state/actions'
import {useT} from '@/state/store'

const GROUPS = [
  {key: 'perm.owner', shift: 6},
  {key: 'perm.group', shift: 3},
  {key: 'perm.other', shift: 0}
] as const

const BITS = [
  {key: 'perm.read', bit: 4},
  {key: 'perm.write', bit: 2},
  {key: 'perm.execute', bit: 1}
] as const

export function PermissionsDialog({
  tabId,
  entries,
  close
}: {
  tabId: string,
  entries: Entry[],
  close: () => void
}) {
  const t = useT()
  const initial = entries.find((e) => e.permissions !== null)?.permissions ?? 0o644
  const [mode, setMode] = useState(initial & 0o777)
  const [octal, setOctal] = useState(toOctal(initial))

  const setBit = (shift: number, bit: number, on: boolean) => {
    const mask = bit << shift
    const next = on ? mode | mask : mode & ~mask
    setMode(next)
    setOctal(toOctal(next))
  }

  const onOctal = (text: string) => {
    setOctal(text)
    const parsed = fromOctal(text)

    if (parsed !== null) {
      setMode(parsed & 0o777)
    }
  }

  return (
    <Modal
      title={t('perm.title')}
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('dialog.cancel')}</button>
          <button
            className="primary"
            onClick={() => {
              close()
              void applyPermissions(tabId, entries, mode)
            }}
          >
            {t('perm.apply')}
          </button>
        </>
      }
    >
      <p className="hint">
        {entries.length === 1 ? entries[0].path : t('perm.files', {n: entries.length})}
      </p>
      <div className="perm-grid">
        <span />
        {BITS.map((b) => (
          <span key={b.key} className="h">
            {t(b.key)}
          </span>
        ))}
        {GROUPS.map((group) => (
          <FragmentRow key={group.key}>
            <span className="h">{t(group.key)}</span>
            {BITS.map((b) => (
              <input
                key={b.key}
                type="checkbox"
                checked={Boolean(mode & (b.bit << group.shift))}
                onChange={(e) => setBit(group.shift, b.bit, e.target.checked)}
              />
            ))}
          </FragmentRow>
        ))}
      </div>
      <label>
        <span className="hint">{t('perm.octal')}</span>
        <input
          type="text"
          value={octal}
          maxLength={4}
          style={{width: 80, fontFamily: 'var(--mono)'}}
          onChange={(e) => onOctal(e.target.value.replace(/[^0-7]/g, ''))}
        />
      </label>
    </Modal>
  )
}

function FragmentRow({children}: {children: React.ReactNode}) {
  return <>{children}</>
}