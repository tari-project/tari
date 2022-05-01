import { InputProps } from '../Input/types'

export interface TextInputProps extends Omit<InputProps, 'type'> {
  hideText?: boolean
}
