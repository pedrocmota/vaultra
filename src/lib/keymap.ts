export interface KeyChord {
  key: string,
  ctrl: boolean,
  shift: boolean,
  alt: boolean
}

export interface KeyBinding {
  keys: string | string[],
  inInputs?: boolean
}

export type KeymapSpec<C extends string> = Record<C, string | string[] | KeyBinding>

export type CommandHandlers<C extends string> = Record<C, () => unknown>

export interface ResolveOptions {
  inInput?: boolean
}

type KeyboardLike = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'shiftKey' | 'altKey' | 'metaKey'>

const KEY_ALIASES: Record<string, string> = {
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  Delete: 'Del',
  Escape: 'Esc',
  ' ': 'Space'
}

const MODIFIERS = new Set(['ctrl', 'shift', 'alt'])

function canonicalKey(key: string): string {
  const aliased = KEY_ALIASES[key] ?? key

  return aliased.length === 1 ? aliased.toLowerCase() : aliased
}

function chordId(chord: KeyChord): string {
  return `${chord.ctrl ? 'ctrl+' : ''}${chord.alt ? 'alt+' : ''}${chord.shift ? 'shift+' : ''}${chord.key}`
}

function normalizeBinding(
  entry: string | string[] | KeyBinding
): Required<KeyBinding> & { keys: string[] } {
  const binding = typeof entry === 'string' || Array.isArray(entry) ? { keys: entry } : entry
  const keys = Array.isArray(binding.keys) ? binding.keys : [binding.keys]

  return { keys, inInputs: binding.inInputs ?? false }
}

export function parseChord(shortcut: string): KeyChord {
  const parts = shortcut.split('+')
  const key = parts.pop() ?? ''
  const modifiers = new Set(parts.map((part) => part.toLowerCase()))
  const unknown = [...modifiers].find((modifier) => !MODIFIERS.has(modifier))

  if (unknown) {
    throw new Error(`Unknown modifier "${unknown}" in shortcut "${shortcut}"`)
  }

  return {
    key: canonicalKey(key),
    ctrl: modifiers.has('ctrl'),
    shift: modifiers.has('shift'),
    alt: modifiers.has('alt')
  }
}

export function chordFromEvent(event: KeyboardLike): KeyChord {
  return {
    key: canonicalKey(event.key),
    ctrl: event.ctrlKey || event.metaKey,
    shift: event.shiftKey,
    alt: event.altKey
  }
}

export function isTextInput(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) {
    return false
  }

  return target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable
}

export class Keymap<C extends string> {
  private readonly byChord = new Map<string, { command: C, inInputs: boolean }>()

  private readonly labels = new Map<C, string>()

  public constructor(spec: KeymapSpec<C>) {
    for (const [command, entry] of Object.entries(spec) as [C, string | string[] | KeyBinding][]) {
      const binding = normalizeBinding(entry)

      for (const shortcut of binding.keys) {
        const id = chordId(parseChord(shortcut))

        if (this.byChord.has(id)) {
          throw new Error(`Shortcut "${shortcut}" is bound more than once`)
        }

        this.byChord.set(id, { command, inInputs: binding.inInputs })
      }

      this.labels.set(command, binding.keys[0])
    }
  }

  public resolve(event: KeyboardLike, options: ResolveOptions = {}): C | null {
    const match = this.byChord.get(chordId(chordFromEvent(event)))

    if (!match || (options.inInput && !match.inInputs)) {
      return null
    }

    return match.command
  }

  public label(command: C): string {
    return this.labels.get(command) ?? ''
  }

  public dispatch(
    event: KeyboardLike & { preventDefault: () => void },
    handlers: CommandHandlers<C>,
    options: ResolveOptions = {}
  ): boolean {
    const command = this.resolve(event, options)

    if (!command) {
      return false
    }

    event.preventDefault()
    void handlers[command]()

    return true
  }
}

export function createKeymap<C extends string>(spec: KeymapSpec<C>): Keymap<C> {
  return new Keymap(spec)
}