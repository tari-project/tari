import { ReactNode } from 'react'

export interface SelectInternalProps {
  disabled?: boolean
  inverted?: boolean
  children?: ReactNode
  open?: boolean
}

export interface Option {
  value: string
  label: string
  key: string
}

export interface SelectProps {
  keys: string[]
  disabled?: boolean
  inverted?: boolean
  label: string
  value?: Option
  options: Option[]
  onChange: (option: Option) => void
}
