import { ReactNode } from 'react'

export type TextInputTypes = 'static' | 'disabled'

export interface TextInputProps {
  type?: TextInputTypes
  value?: string
  hideText?: boolean
  placeholder?: string
  inputIcon?: ReactNode
  inputUnits?: string
  onIconClick?: () => void
  onChangeText?: (value: string) => void
  testId?: string
}
