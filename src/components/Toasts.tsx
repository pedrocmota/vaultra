import {useEffect} from 'react'
import {useStore} from '@/state/store'

export function Toasts() {
  const toasts = useStore((s) => s.toasts)
  const dismiss = useStore((s) => s.dismissToast)

  useEffect(() => {
    if (toasts.length === 0) {
      return
    }

    const timer = window.setTimeout(
      () => dismiss(toasts[0].id),
      toasts[0].kind === 'error' ? 8000 : 3500
    )

    return () => window.clearTimeout(timer)
  }, [toasts, dismiss])

  if (toasts.length === 0) {
    return null
  }

  return (
    <div className="toast-area">
      {toasts.map((toast) => (
        <div key={toast.id} className={`toast ${toast.kind}`} onClick={() => dismiss(toast.id)}>
          {toast.text}
        </div>
      ))}
    </div>
  )
}