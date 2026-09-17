export interface DragSession {
  label: string,
  onDrop: (target: Element | null, clientX: number, clientY: number) => void,
  onMove?: (target: Element | null) => void
}

const THRESHOLD = 6

let ghost: HTMLDivElement | null = null

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

  const onMove = (e: MouseEvent) => {
    if (!active) {
      if (Math.abs(e.clientX - startX) < THRESHOLD && Math.abs(e.clientY - startY) < THRESHOLD) {
        return
      }

      active = true
      ghost = createGhost(session.label)
      document.body.style.cursor = 'grabbing'
    }

    if (ghost) {
      ghost.style.left = `${e.clientX}px`
      ghost.style.top = `${e.clientY}px`
    }

    session.onMove?.(elementUnder(e.clientX, e.clientY))
  }

  const onUp = (e: MouseEvent) => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)

    if (!active) {
      return
    }

    const target = elementUnder(e.clientX, e.clientY)
    ghost?.remove()
    ghost = null
    document.body.style.cursor = ''
    session.onMove?.(null)
    session.onDrop(target, e.clientX, e.clientY)
  }

  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
}