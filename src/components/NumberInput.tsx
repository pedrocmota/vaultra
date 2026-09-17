import {useEffect, useState} from 'react'

interface Props {
  value: number,
  min?: number,
  max?: number,
  step?: number,
  fallback?: number,
  style?: React.CSSProperties,
  onCommit: (value: number) => void
}

function parseNumber(raw: string): number | null {
  if (raw.trim() === '') {
    return null
  }

  const parsed = Number(raw)

  return Number.isFinite(parsed) ? parsed : null
}

export function NumberInput({
  value,
  min = 0,
  max = Number.MAX_SAFE_INTEGER,
  step,
  fallback,
  style,
  onCommit
}: Props) {
  const [text, setText] = useState(String(value))

  useEffect(() => {
    if (parseNumber(text) !== value) {
      setText(String(value))
    }
  }, [value])

  const handleChange = (raw: string) => {
    setText(raw)
    const parsed = parseNumber(raw)

    if (parsed !== null && parsed >= min && parsed <= max) {
      onCommit(parsed)
    }
  }

  const handleBlur = () => {
    const parsed = parseNumber(text)
    const committed = parsed === null ? (fallback ?? min) : Math.min(max, Math.max(min, parsed))
    setText(String(committed))

    if (committed !== value) {
      onCommit(committed)
    }
  }

  return (
    <input
      type="number"
      min={min}
      max={max}
      step={step}
      style={style}
      value={text}
      onChange={(e) => handleChange(e.target.value)}
      onBlur={handleBlur}
    />
  )
}