import Input from '../Input'

import { TextInputProps } from './types'

/**
 * @name TextInput component
 * @typedef TextInputProps
 *
 * @prop {boolean} [disabled] - whether it is disabled or not
 * @prop {string} [value] - input text value
 * @prop {boolean} [hideText] - show/hide input text
 * @prop {string} [placeholder] - placeholder text
 * @prop {ReactNode} [inputIcon] - optional icon rendered inside input field
 * @prop {string} [inputUnits] - optional units text, e.g. 'MB' on right-hand side of input field
 * @prop {() => void} [onIconClick] - icon click event
 * @prop {(value: string) => void} [onChange] - text change event handler
 * @prop {string} [testId] - for testing purposes
 */

const TextInput = ({ hideText = false, value, ...props }: TextInputProps) => {
  return <Input type='text' value={hideText ? '' : value} {...props} />
}

export default TextInput
