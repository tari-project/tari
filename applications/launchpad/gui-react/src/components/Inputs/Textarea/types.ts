import { TextareaHTMLAttributes } from 'react'

export interface TextareaProps
  extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, 'onChange'> {
  label?: string
  onChange?: (value: string) => void
  inverted?: boolean
  testId?: string
  error?: string
  withError?: boolean
}
