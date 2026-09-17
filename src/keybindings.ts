import {createKeymap} from '@/lib/keymap'

export type GlobalCommand =
  | 'newTab'
  | 'closeTab'
  | 'siteManager'
  | 'nextTab'
  | 'previousTab'
  | 'refresh'

export const globalKeymap = createKeymap<GlobalCommand>({
  newTab: 'Ctrl+T',
  closeTab: 'Ctrl+W',
  siteManager: 'Ctrl+S',
  nextTab: 'Ctrl+Tab',
  previousTab: 'Ctrl+Shift+Tab',
  refresh: ['F5', 'Ctrl+R']
})

export const browserShortcuts = createKeymap<'blocked'>({
  blocked: ['F3', 'F5', 'F7', 'Ctrl+R', 'Ctrl+Shift+R', 'Ctrl+F', 'Ctrl+P', 'Ctrl+U', 'Ctrl+S']
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
  | 'copy'
  | 'paste'
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
  rename: {keys: 'F2', inInputs: true},
  viewEdit: {keys: 'F3', inInputs: true},
  transfer: {keys: 'F6', inInputs: true},
  newFolder: {keys: 'F7', inInputs: true},
  selectAll: 'Ctrl+A',
  copy: 'Ctrl+C',
  paste: 'Ctrl+V',
  refresh: {keys: ['F5', 'Ctrl+R'], inInputs: true},
  historyBack: 'Alt+Left',
  historyForward: 'Alt+Right'
})