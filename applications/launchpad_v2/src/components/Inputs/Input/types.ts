import { ReactNode, CSSProperties } from 'react'

export interface InputProps {
  type?: string
  disabled?: boolean
  value?: string
  placeholder?: string
  inputUnits?: string
  inputIcon?: ReactNode
  onIconClick?: () => void
  onChange?: (value: string) => void
  testId?: string
  style?: CSSProperties
}
