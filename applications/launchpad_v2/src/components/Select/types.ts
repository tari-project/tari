import { ReactNode } from 'react'

export type SelectInternalProps = {
  disabled?: boolean
  inverted?: boolean
  children?: ReactNode
  open?: boolean
}

type Option = { value: string; label: string; key: string }
export type SelectProps = {
  disabled?: boolean
  inverted?: boolean
  label: string
  value?: Option
  options: Option[]
  onChange: (option: Option) => void
}
