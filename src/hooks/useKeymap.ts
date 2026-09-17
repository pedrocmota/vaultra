import { useEffect, useRef } from 'react'
import type { CommandHandlers, Keymap } from '@/lib/keymap'

export function useGlobalKeymap<C extends string>(
  keymap: Keymap<C>,
  handlers: CommandHandlers<C>,
  enabled = true
): void {
  const handlersRef = useRef(handlers)
  handlersRef.current = handlers

  useEffect(() => {
    if (!enabled) {
      return
    }

    const onKeyDown = (event: KeyboardEvent) => {
      keymap.dispatch(event, handlersRef.current)
    }

    window.addEventListener('keydown', onKeyDown)

    return () => window.removeEventListener('keydown', onKeyDown)
  }, [keymap, enabled])
}