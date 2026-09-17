const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) {
    return ''
  }

  if (bytes < 1024) {
    return `${bytes} B`
  }

  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024
    unit += 1
  }

  return `${value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2)} ${UNITS[unit]}`
}

export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`
}

export function formatDate(unixSeconds: number | null | undefined, language: string): string {
  if (!unixSeconds) {
    return ''
  }

  const date = new Date(unixSeconds * 1000)

  return date.toLocaleString(language, {
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit'
  })
}

export function formatDateTimeMs(unixMillis: number | null | undefined, language: string): string {
  if (!unixMillis) {
    return ''
  }

  return new Date(unixMillis).toLocaleString(language, {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit'
  })
}

export function formatTime(unixMillis: number, language: string): string {
  return new Date(unixMillis).toLocaleTimeString(language, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit'
  })
}

export function formatPermissions(mode: number | null | undefined, isDir = false): string {
  if (mode === null || mode === undefined) {
    return ''
  }

  const bits = ['r', 'w', 'x']
  let out = isDir ? 'd' : '-'
  for (let group = 2; group >= 0; group -= 1) {
    for (let bit = 2; bit >= 0; bit -= 1) {
      out += mode & (1 << (group * 3 + bit)) ? bits[2 - bit] : '-'
    }
  }

  return out
}

export function toOctal(mode: number): string {
  return (mode & 0o777).toString(8).padStart(3, '0')
}

export function fromOctal(text: string): number | null {
  if (!/^[0-7]{3,4}$/.test(text)) {
    return null
  }

  return parseInt(text, 8)
}

export function percent(transferred: number, size: number | null): number {
  if (!size || size <= 0) {
    return 0
  }

  return Math.min(100, Math.round((transferred / size) * 100))
}

export function basename(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, '')
  const index = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'))

  return index >= 0 ? trimmed.slice(index + 1) : trimmed
}

export function joinRemote(base: string, name: string): string {
  if (name.startsWith('/')) {
    return name
  }

  if (!base || base === '/') {
    return `/${name}`
  }

  return base.endsWith('/') ? `${base}${name}` : `${base}/${name}`
}

export function joinLocal(base: string, name: string): string {
  if (!base) {
    return name
  }

  return base.endsWith('\\') || base.endsWith('/') ? `${base}${name}` : `${base}\\${name}`
}

export function parentRemote(path: string): string {
  const trimmed = path.replace(/\/+$/, '')
  const index = trimmed.lastIndexOf('/')

  if (index <= 0) {
    return '/'
  }

  return trimmed.slice(0, index)
}

export function relativeRemote(base: string, path: string): string | null {
  const normalizedBase = base.replace(/\/+$/, '')

  if (path === normalizedBase) {
    return ''
  }

  if (!path.startsWith(`${normalizedBase}/`)) {
    return null
  }

  return path.slice(normalizedBase.length + 1)
}

export function relativeLocal(base: string, path: string): string | null {
  const normalizedBase = base.replace(/[\\/]+$/, '').toLowerCase()
  const lower = path.toLowerCase()

  if (lower === normalizedBase) {
    return ''
  }

  if (!lower.startsWith(`${normalizedBase}\\`)) {
    return null
  }

  return path.slice(normalizedBase.length + 1).replace(/\\/g, '/')
}

export function buildUrl(
  site: { protocol: string, host: string, port: number, user: string, logonType: string },
  remotePath: string
): string {
  const scheme = site.protocol === 'sftp' ? 'sftp' : site.protocol === 'ftp' ? 'ftp' : 'ftps'
  const defaultPort = site.protocol === 'sftp' ? 22 : site.protocol === 'ftps_implicit' ? 990 : 21
  const auth =
    site.user && site.logonType !== 'anonymous' ? `${encodeURIComponent(site.user)}@` : ''

  const port = site.port && site.port !== defaultPort ? `:${site.port}` : ''
  const path = remotePath.split('/').map(encodeURIComponent).join('/')

  return `${scheme}://${auth}${site.host}${port}${path}`
}