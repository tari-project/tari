import { ReactNode, CSSProperties, InputHTMLAttributes } from 'react'

export interface InputProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'onChange'> {
  type?: string
  disabled?: boolean
  value?: string
  id?: string
  label?: string
  error?: string
  placeholder?: string
  inputUnits?: string
  inputIcon?: ReactNode
  onIconClick?: () => void
  onChange?: (value: string) => void
  testId?: string
  style?: CSSProperties
  containerStyle?: CSSProperties
  inverted?: boolean
  withError?: boolean
  onClick?: () => void
}
