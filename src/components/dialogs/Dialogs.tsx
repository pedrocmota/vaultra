import {useEffect, useRef, useState} from 'react'
import {Modal} from '@/components/Modal'
import type {
  ConflictAction,
  ConflictPrompt,
  EditedFileChanged,
  HostKeyChangedDetail,
  SiteConfig,
  UiPrompt,
  UntrustedCertificateDetail
} from '@/lib/api'
import {basename, formatBytes, formatDate} from '@/lib/format'
import {
  acceptCertificate,
  acceptChangedHostKey,
  answerConflict,
  answerPrompt,
  connectSite,
  uploadEditedFile
} from '@/state/actions'
import {useStore, useT, type Dialog} from '@/state/store'
import {PermissionsDialog} from './PermissionsDialog'
import {SettingsDialog} from './SettingsDialog'
import {SiteManager} from './SiteManager'
import {SyncDialog} from './SyncDialog'

export function Dialogs() {
  const dialogs = useStore((s) => s.dialogs)
  const closeDialog = useStore((s) => s.closeDialog)
  const dialog = dialogs[dialogs.length - 1]

  if (!dialog) {
    return null
  }

  return <DialogSwitch key={dialogs.length} dialog={dialog} close={closeDialog} />
}

function DialogSwitch({dialog, close}: {dialog: Dialog, close: () => void}) {
  switch (dialog.kind) {
    case 'prompt':
      return <PromptDialog prompt={dialog.prompt} close={close} />
    case 'password':
      return <PasswordDialog site={dialog.site} tabId={dialog.tabId} close={close} />
    case 'hostKeyChanged':
      return (
        <HostKeyChangedDialog
          detail={dialog.detail}
          site={dialog.site}
          tabId={dialog.tabId}
          password={dialog.password}
          close={close}
        />
      )
    case 'certificate':
      return (
        <CertificateDialog
          detail={dialog.detail}
          site={dialog.site}
          tabId={dialog.tabId}
          password={dialog.password}
          close={close}
        />
      )
    case 'conflict':
      return <ConflictDialog prompt={dialog.prompt} close={close} />
    case 'input':
      return (
        <InputDialog
          title={dialog.title}
          label={dialog.label}
          initial={dialog.initial}
          selectStem={dialog.selectStem}
          onSubmit={dialog.onSubmit}
          close={close}
        />
      )
    case 'confirm':
      return (
        <ConfirmDialog
          title={dialog.title}
          message={dialog.message}
          danger={dialog.danger}
          confirmLabel={dialog.confirmLabel}
          onConfirm={dialog.onConfirm}
          close={close}
        />
      )
    case 'permissions':
      return <PermissionsDialog tabId={dialog.tabId} entries={dialog.entries} close={close} />
    case 'siteManager':
      return <SiteManager selectId={dialog.selectId} close={close} />
    case 'settings':
      return <SettingsDialog close={close} />
    case 'sync':
      return <SyncDialog tabId={dialog.tabId} close={close} />
    case 'about':
      return <AboutDialog close={close} />
    case 'editChanged':
      return <EditChangedDialog event={dialog.event} close={close} />
    default:
      return null
  }
}

function PromptDialog({prompt, close}: {prompt: UiPrompt, close: () => void}) {
  const t = useT()
  const [value, setValue] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const siteName = useStore(
    (s) => s.tabs.find((x) => x.sessionId === prompt.sessionId || x.connecting)?.site?.host ?? ''
  )

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  const answer = (text: string | null) => {
    void answerPrompt(prompt.promptId, text)
    close()
  }

  if (prompt.kind === 'host_key') {
    return (
      <Modal
        title={t('prompt.hostkey.title')}
        tone="warning"
        onClose={() => answer('no')}
        footer={
          <>
            <button onClick={() => answer('no')}>{t('prompt.hostkey.reject')}</button>
            <button className="primary" onClick={() => answer('yes')}>
              {t('prompt.hostkey.accept')}
            </button>
          </>
        }
      >
        <p>{t('prompt.hostkey.message')}</p>
        <div className="kv">
          <span className="k">{t('prompt.hostkey.host')}</span>
          <span>{prompt.host || siteName}</span>
          <span className="k">{t('prompt.hostkey.type')}</span>
          <span>{prompt.keyType}</span>
          <span className="k">{t('prompt.hostkey.fingerprint')}</span>
          <span className="mono">{prompt.fingerprint}</span>
        </div>
        <details>
          <summary className="hint">OpenSSH</summary>
          <div className="mono">{prompt.text}</div>
        </details>
      </Modal>
    )
  }

  if (prompt.kind === 'confirm') {
    return (
      <Modal
        title={t('app.title')}
        onClose={() => answer('no')}
        footer={
          <>
            <button onClick={() => answer('no')}>{t('prompt.confirm.no')}</button>
            <button className="primary" onClick={() => answer('yes')}>
              {t('prompt.confirm.yes')}
            </button>
          </>
        }
      >
        <p className="mono">{prompt.text}</p>
      </Modal>
    )
  }

  return (
    <Modal
      title={siteName ? t('prompt.passwordFor', {target: siteName}) : t('prompt.password')}
      onClose={() => answer(null)}
      footer={
        <>
          <button onClick={() => answer(null)}>{t('prompt.cancel')}</button>
          <button className="primary" onClick={() => answer(value)}>
            {t('prompt.submit')}
          </button>
        </>
      }
    >
      <p className="mono">{prompt.text.trim() || t('prompt.password')}</p>
      <input
        ref={inputRef}
        type="password"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            answer(value)
          }
        }}
      />
    </Modal>
  )
}

function PasswordDialog({
  site,
  tabId,
  close
}: {
  site: SiteConfig,
  tabId: string,
  close: () => void
}) {
  const t = useT()
  const [value, setValue] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  useEffect(() => inputRef.current?.focus(), [])
  const submit = () => {
    close()
    void connectSite(site, {tabId, password: value})
  }

  return (
    <Modal
      title={t('prompt.passwordFor', {target: `${site.user ? `${site.user}@` : ''}${site.host}`})}
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('prompt.cancel')}</button>
          <button className="primary" onClick={submit}>
            {t('sm.connect')}
          </button>
        </>
      }
    >
      <input
        ref={inputRef}
        type="password"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => e.key === 'Enter' && submit()}
      />
    </Modal>
  )
}

function HostKeyChangedDialog({
  detail,
  site,
  tabId,
  password,
  close
}: {
  detail: HostKeyChangedDetail,
  site: SiteConfig,
  tabId: string,
  password: string | null,
  close: () => void
}) {
  const t = useT()

  return (
    <Modal
      title={t('prompt.hostkeyChanged.title')}
      tone="danger"
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('prompt.cancel')}</button>
          <button
            className="danger"
            onClick={() => {
              close()
              void acceptChangedHostKey(tabId, site, detail, password)
            }}
          >
            {t('prompt.hostkeyChanged.accept')}
          </button>
        </>
      }
    >
      <p>{t('prompt.hostkeyChanged.message')}</p>
      <div className="kv">
        <span className="k">{t('prompt.hostkey.host')}</span>
        <span>{detail.host}</span>
        <span className="k">{t('prompt.hostkey.type')}</span>
        <span>{detail.keyType}</span>
        <span className="k">{t('prompt.hostkey.fingerprint')}</span>
        <span className="mono">{detail.fingerprint}</span>
        <span className="k">{t('prompt.hostkeyChanged.line')}</span>
        <span className="mono">{detail.knownHostsLine}</span>
      </div>
    </Modal>
  )
}

function CertificateDialog({
  detail,
  site,
  tabId,
  password,
  close
}: {
  detail: UntrustedCertificateDetail,
  site: SiteConfig,
  tabId: string,
  password: string | null,
  close: () => void
}) {
  const t = useT()
  const [remember, setRemember] = useState(false)
  const fingerprint =
    detail.fingerprint.toUpperCase().match(/.{2}/g)?.join(':') ?? detail.fingerprint

  return (
    <Modal
      title={t('prompt.cert.title')}
      tone="warning"
      size="wide"
      onClose={close}
      footer={
        <>
          <label className="left">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
            />
            {t('prompt.cert.remember')}
          </label>
          <button onClick={close}>{t('prompt.cancel')}</button>
          <button
            className="primary"
            onClick={() => {
              close()
              void acceptCertificate(tabId, site, detail.fingerprint, remember, password)
            }}
          >
            {t('prompt.cert.accept')}
          </button>
        </>
      }
    >
      <p>{t('prompt.cert.message')}</p>
      <div className="kv">
        <span className="k">{t('prompt.hostkey.host')}</span>
        <span>{detail.host}</span>
        <span className="k">{t('prompt.cert.subject')}</span>
        <span>{detail.subject}</span>
        <span className="k">{t('prompt.cert.issuer')}</span>
        <span>{detail.issuer}</span>
        <span className="k">{t('prompt.cert.validity')}</span>
        <span>
          {detail.notBefore} → {detail.notAfter}
        </span>
        <span className="k">{t('prompt.cert.reason')}</span>
        <span>{detail.reason}</span>
        <span className="k">{t('prompt.cert.fingerprint')}</span>
        <span className="mono">{fingerprint}</span>
      </div>
    </Modal>
  )
}

function ConflictDialog({prompt, close}: {prompt: ConflictPrompt, close: () => void}) {
  const t = useT()
  const language = useStore((s) => s.language)
  const [applyAll, setApplyAll] = useState(false)
  const choose = (action: ConflictAction) => {
    void answerConflict(prompt.itemId, action, applyAll)
    close()
  }

  const actions: ConflictAction[] = ['overwrite', 'overwrite_if_newer', 'resume', 'rename', 'skip']
  const labels: Record<ConflictAction, string> = {
    overwrite: t('conflict.overwrite'),
    overwrite_if_newer: t('conflict.overwriteIfNewer'),
    resume: t('conflict.resume'),
    rename: t('conflict.rename'),
    skip: t('conflict.skip')
  }

  return (
    <Modal
      title={t('conflict.title')}
      size="wide"
      onClose={() => choose('skip')}
      footer={
        <>
          <label className="left">
            <input
              type="checkbox"
              checked={applyAll}
              onChange={(e) => setApplyAll(e.target.checked)}
            />
            {t('conflict.applyAll')}
          </label>
          {actions.map((action) => (
            <button
              key={action}
              className={action === 'overwrite' ? 'primary' : ''}
              onClick={() => choose(action)}
            >
              {labels[action]}
            </button>
          ))}
        </>
      }
    >
      <p>{t('conflict.message')}</p>
      <div className="kv">
        <span className="k">{t('conflict.source')}</span>
        <span className="mono">
          {prompt.sourcePath}
          <br />
          {formatBytes(prompt.sourceSize)} · {formatDate(prompt.sourceMtime, language)}
        </span>
        <span className="k">{t('conflict.target')}</span>
        <span className="mono">
          {prompt.targetPath}
          <br />
          {formatBytes(prompt.targetSize)} · {formatDate(prompt.targetMtime, language)}
        </span>
      </div>
    </Modal>
  )
}

function InputDialog({
  title,
  label,
  initial,
  selectStem,
  onSubmit,
  close
}: {
  title: string,
  label: string,
  initial: string,
  selectStem?: boolean,
  onSubmit: (value: string) => void | Promise<void>,
  close: () => void
}) {
  const t = useT()
  const [value, setValue] = useState(initial)
  const inputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    const input = inputRef.current

    if (!input) {
      return
    }

    input.focus()

    if (selectStem) {
      const dot = initial.lastIndexOf('.')
      input.setSelectionRange(0, dot > 0 ? dot : initial.length)
    } else {
      input.select()
    }
  }, [initial, selectStem])
  const submit = () => {
    if (!value.trim()) {
      return
    }

    close()
    void onSubmit(value.trim())
  }

  return (
    <Modal
      title={title}
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('dialog.cancel')}</button>
          <button className="primary" onClick={submit} disabled={!value.trim()}>
            {t('dialog.ok')}
          </button>
        </>
      }
    >
      <label style={{flexDirection: 'column', alignItems: 'stretch'}}>
        <span className="hint">{label}</span>
        <input
          ref={inputRef}
          type="text"
          value={value}
          spellCheck={false}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && submit()}
        />
      </label>
    </Modal>
  )
}

function ConfirmDialog({
  title,
  message,
  danger,
  confirmLabel,
  onConfirm,
  close
}: {
  title: string,
  message: string,
  danger?: boolean,
  confirmLabel?: string,
  onConfirm: () => void | Promise<void>,
  close: () => void
}) {
  const t = useT()
  const buttonRef = useRef<HTMLButtonElement>(null)
  useEffect(() => buttonRef.current?.focus(), [])

  return (
    <Modal
      title={title}
      tone={danger ? 'danger' : 'normal'}
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('dialog.cancel')}</button>
          <button
            ref={buttonRef}
            className={danger ? 'danger' : 'primary'}
            onClick={() => {
              close()
              void onConfirm()
            }}
          >
            {confirmLabel ?? t('dialog.ok')}
          </button>
        </>
      }
    >
      <p>{message}</p>
    </Modal>
  )
}

function AboutDialog({close}: {close: () => void}) {
  const t = useT()
  const system = useStore((s) => s.system)

  return (
    <Modal
      title={t('about.title')}
      onClose={close}
      footer={
        <button className="primary" onClick={close}>
          {t('dialog.close')}
        </button>
      }
    >
      <p>
        <strong>Vaultra</strong> — {t('about.tagline')}
      </p>
      <div className="kv">
        <span className="k">{t('about.version')}</span>
        <span>{system?.version}</span>
        <span className="k">{t('about.openssh')}</span>
        <span>{system?.openssh ?? t('about.notFound')}</span>
        <span className="k">{t('about.dataDir')}</span>
        <span className="mono">{system?.dataDir}</span>
        <span className="k">{t('about.logDir')}</span>
        <span className="mono">{system?.logDir}</span>
      </div>
    </Modal>
  )
}

function EditChangedDialog({event, close}: {event: EditedFileChanged, close: () => void}) {
  const t = useT()

  return (
    <Modal
      title={t('edit.changedTitle')}
      onClose={close}
      footer={
        <>
          <button onClick={close}>{t('edit.ignore')}</button>
          <button
            className="primary"
            onClick={() => {
              close()
              void uploadEditedFile(event)
            }}
          >
            {t('edit.upload')}
          </button>
        </>
      }
    >
      <p>{t('edit.changedMessage', {name: basename(event.remotePath)})}</p>
      <div className="mono">{event.remotePath}</div>
    </Modal>
  )
}