export interface DragSession {
  label: string,
  onDrop: (target: Element | null, clientX: number, clientY: number) => void,
  onMove?: (target: Element | null) => void,
  onLeaveWindow?: (event: MouseEvent) => void
}

const THRESHOLD = 6

let ghost: HTMLDivElement | null = null

function isOutsideWindow(event: MouseEvent): boolean {
  const left = window.screenX
  const top = window.screenY

  return (
    event.screenX < left
    || event.screenY < top
    || event.screenX >= left + window.outerWidth
    || event.screenY >= top + window.outerHeight
  )
}

function createGhost(label: string): HTMLDivElement {
  const element = document.createElement('div')
  element.textContent = label
  Object.assign(element.style, {
    position: 'fixed',
    zIndex: '1000',
    pointerEvents: 'none',
    padding: '4px 10px',
    borderRadius: '6px',
    background: 'var(--accent)',
    color: 'var(--accent-fg)',
    fontSize: '12px',
    boxShadow: 'var(--shadow)',
    opacity: '0.92',
    transform: 'translate(12px, 12px)'
  })
  document.body.appendChild(element)

  return element
}

export function startMouseDrag(event: MouseEvent | React.MouseEvent, session: DragSession) {
  if (event.button !== 0) {
    return
  }

  const startX = event.clientX
  const startY = event.clientY
  let active = false

  const elementUnder = (x: number, y: number) => {
    if (ghost) {
      ghost.style.display = 'none'
    }

    const element = document.elementFromPoint(x, y)

    if (ghost) {
      ghost.style.display = ''
    }

    return element
  }

  const finish = () => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
    ghost?.remove()
    ghost = null
    document.body.style.cursor = ''
    session.onMove?.(null)
  }

  function onMove(e: MouseEvent) {
    if (!active) {
      if (Math.abs(e.clientX - startX) < THRESHOLD && Math.abs(e.clientY - startY) < THRESHOLD) {
        return
      }

      active = true
      ghost = createGhost(session.label)
      document.body.style.cursor = 'grabbing'
    }

    if (session.onLeaveWindow && isOutsideWindow(e)) {
      finish()
      session.onLeaveWindow(e)

      return
    }

    if (ghost) {
      ghost.style.left = `${e.clientX}px`
      ghost.style.top = `${e.clientY}px`
    }

    session.onMove?.(elementUnder(e.clientX, e.clientY))
  }

  function onUp(e: MouseEvent) {
    if (!active) {
      window.removeEventListener('mousemove', onMove)
      window.removeEventListener('mouseup', onUp)

      return
    }

    const target = elementUnder(e.clientX, e.clientY)
    finish()
    session.onDrop(target, e.clientX, e.clientY)
  }

  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}