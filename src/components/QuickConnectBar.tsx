import {useState} from 'react'
import {defaultPort, defaultSite, type Protocol} from '@/lib/api'
import {connectSite} from '@/state/actions'
import {useT} from '@/state/store'

export function QuickConnectBar() {
  const t = useT()
  const [protocol, setProtocol] = useState<Protocol>('sftp')
  const [host, setHost] = useState('')
  const [port, setPort] = useState('')
  const [user, setUser] = useState('')
  const [password, setPassword] = useState('')
  const protocols: {value: Protocol, label: string}[] = [
    {value: 'sftp', label: t('proto.sftp')},
    {value: 'ftp', label: t('proto.ftp')},
    {value: 'ftps_explicit', label: t('proto.ftpsExplicit')},
    {value: 'ftps_implicit', label: t('proto.ftpsImplicit')}
  ]

  const connect = () => {
    const trimmedHost = host.trim()

    if (!trimmedHost) {
      return
    }

    const parsedPort = Number(port) || defaultPort(protocol)
    const logonType = password
      ? 'normal'
      : protocol === 'sftp'
        ? 'interactive'
        : user.trim()
          ? 'ask'
          : 'anonymous'

    const site = defaultSite({
      name: trimmedHost,
      protocol,
      host: trimmedHost,
      port: parsedPort,
      user: user.trim(),
      logonType
    })
    void connectSite(site, {
      password: password || (logonType === 'interactive' ? null : undefined)
    })
    setPassword('')
  }

  const onKey = (event: React.KeyboardEvent) => {
    if (event.key === 'Enter') {
      connect()
    }
  }

  return (
    <div className="quickbar">
      <select value={protocol} onChange={(e) => setProtocol(e.target.value as Protocol)}>
        {protocols.map((p) => (
          <option key={p.value} value={p.value}>
            {p.label}
          </option>
        ))}
      </select>
      <input
        type="text"
        placeholder={t('quick.host')}
        value={host}
        onChange={(e) => setHost(e.target.value)}
        onKeyDown={onKey}
        spellCheck={false}
      />
      <input
        type="text"
        placeholder={t('quick.user')}
        value={user}
        onChange={(e) => setUser(e.target.value)}
        onKeyDown={onKey}
        spellCheck={false}
      />
      <input
        type="password"
        placeholder={t('quick.password')}
        value={password}
        onChange={(e) => setPassword(e.target.value)}
        onKeyDown={onKey}
      />
      <input
        className="small"
        type="text"
        placeholder={String(defaultPort(protocol))}
        value={port}
        onChange={(e) => setPort(e.target.value.replace(/\D/g, ''))}
        onKeyDown={onKey}
      />
      <button className="primary" onClick={connect} disabled={!host.trim()}>
        {t('quick.connect')}
      </button>
    </div>
  )
}