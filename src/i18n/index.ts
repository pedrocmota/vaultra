import {en} from './en'
import {ptBR} from './pt-BR'

export type MessageKey = keyof typeof ptBR
export type Language = 'pt-BR' | 'en'

const dictionaries: Record<Language, Record<MessageKey, string>> = {'pt-BR': ptBR, en}

export const languages: {code: Language, label: string}[] = [
  {code: 'pt-BR', label: 'Português (Brasil)'},
  {code: 'en', label: 'English'}
]

export function normalizeLanguage(value: string | undefined): Language {
  return value === 'en' ? 'en' : 'pt-BR'
}

export function translate(
  language: Language,
  key: MessageKey,
  params?: Record<string, string | number>
): string {
  const template = dictionaries[language][key] ?? dictionaries['pt-BR'][key] ?? key

  if (!params) {
    return template
  }

  return template.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? `{${name}}`))
}