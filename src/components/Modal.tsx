import { useEffect, type ReactNode } from 'react'

interface Props {
  title: ReactNode,
  children: ReactNode,
  footer?: ReactNode,
  onClose: () => void,
  tone?: 'normal' | 'danger' | 'warning',
  size?: 'normal' | 'wide' | 'large',
  closeOnEscape?: boolean
}

export function Modal({
  title,
  children,
  footer,
  onClose,
  tone = 'normal',
  size = 'normal',
  closeOnEscape = true
}: Props) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && closeOnEscape) {
        event.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKey, true)

    return () => window.removeEventListener('keydown', onKey, true)
  }, [onClose, closeOnEscape])

  return (
    <div className="overlay" onMouseDown={(e) => e.stopPropagation()}>
      <div className={`dialog ${size !== 'normal' ? size : ''}`} role="dialog">
        <div className={`head ${tone !== 'normal' ? tone : ''}`}>{title}</div>
        <div className="body">{children}</div>
        {footer && <div className="foot">{footer}</div>}
      </div>
    </div>
  )
}