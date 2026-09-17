import { createKeymap } from '@/lib/keymap'

export type GlobalCommand = 'newTab' | 'closeTab' | 'siteManager' | 'nextTab' | 'previousTab'

export const globalKeymap = createKeymap<GlobalCommand>({
  newTab: 'Ctrl+T',
  closeTab: 'Ctrl+W',
  siteManager: 'Ctrl+S',
  nextTab: 'Ctrl+Tab',
  previousTab: 'Ctrl+Shift+Tab'
})

export type PaneCommand =
  | 'cursorDown'
  | 'cursorUp'
  | 'cursorFirst'
  | 'cursorLast'
  | 'cursorPageDown'
  | 'cursorPageUp'
  | 'selectDown'
  | 'selectUp'
  | 'selectToFirst'
  | 'selectToLast'
  | 'selectPageDown'
  | 'selectPageUp'
  | 'open'
  | 'goUp'
  | 'delete'
  | 'rename'
  | 'viewEdit'
  | 'transfer'
  | 'newFolder'
  | 'selectAll'
  | 'refresh'
  | 'historyBack'
  | 'historyForward'

export const paneKeymap = createKeymap<PaneCommand>({
  cursorDown: 'Down',
  cursorUp: 'Up',
  cursorFirst: 'Home',
  cursorLast: 'End',
  cursorPageDown: 'PageDown',
  cursorPageUp: 'PageUp',
  selectDown: 'Shift+Down',
  selectUp: 'Shift+Up',
  selectToFirst: 'Shift+Home',
  selectToLast: 'Shift+End',
  selectPageDown: 'Shift+PageDown',
  selectPageUp: 'Shift+PageUp',
  open: 'Enter',
  goUp: 'Backspace',
  delete: 'Del',
  rename: { keys: 'F2', inInputs: true },
  viewEdit: { keys: 'F3', inInputs: true },
  transfer: { keys: 'F5', inInputs: true },
  newFolder: { keys: 'F7', inInputs: true },
  selectAll: 'Ctrl+A',
  refresh: 'Ctrl+R',
  historyBack: 'Alt+Left',
  historyForward: 'Alt+Right'
})