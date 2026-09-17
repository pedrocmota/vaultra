import {useEffect, useLayoutEffect, useRef, useState} from 'react'

export interface MenuItem {
  label?: string,
  shortcut?: string,
  disabled?: boolean,
  danger?: boolean,
  separator?: boolean,
  checked?: boolean,
  hidden?: boolean,
  onClick?: () => void
}

interface Props {
  x: number,
  y: number,
  items: MenuItem[],
  onClose: () => void
}

export function ContextMenu({x, y, items, onClose}: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const [position, setPosition] = useState({left: x, top: y})

  useLayoutEffect(() => {
    const element = ref.current

    if (!element) {
      return
    }

    const rect = element.getBoundingClientRect()
    const left = Math.min(x, window.innerWidth - rect.width - 4)
    const top = Math.min(y, window.innerHeight - rect.height - 4)
    setPosition({left: Math.max(0, left), top: Math.max(0, top)})
  }, [x, y])

  useEffect(() => {
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) {
        onClose()
      }
    }

    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose()
      }
    }
    window.addEventListener('mousedown', close, true)
    window.addEventListener('keydown', key, true)
    window.addEventListener('blur', onClose)

    return () => {
      window.removeEventListener('mousedown', close, true)
      window.removeEventListener('keydown', key, true)
      window.removeEventListener('blur', onClose)
    }
  }, [onClose])

  const visible = items.filter((item) => !item.hidden)

  return (
    <div ref={ref} className="ctxmenu" style={position} onContextMenu={(e) => e.preventDefault()}>
      {visible.map((item, index) =>
        item.separator ? (
          <div key={index} className="sep" />
        ) : (
          <div
            key={index}
            className={`item${item.disabled ? ' disabled' : ''}${item.danger ? ' danger' : ''}`}
            onClick={() => {
              if (item.disabled) {
                return
              }

              onClose()
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
  )
}

export interface MenuState {
  x: number,
  y: number,
  items: MenuItem[]
}