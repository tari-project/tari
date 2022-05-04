import { InputProps } from '../Input/types'

export type PasswordInputProps = Omit<
  InputProps,
  'type' | 'inputIcon' | 'onIconClick' | 'inputUnits'
>
