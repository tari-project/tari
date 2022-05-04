import { useState } from 'react'

import Input from '../Input'
import Eye from '../../../styles/Icons/Eye'
import EyeSlash from '../../../styles/Icons/EyeSlash'

import { PasswordInputProps } from './types'

/**
 * @name PasswordInput component
 * @typedef PasswordInputProps
 *
 * @prop {boolean} [disabled] - whether it is disabled or not
 * @prop {string} [value] - input text value
 * @prop {boolean} [hideText] - show/hide input text
 * @prop {string} [placeholder] - placeholder text
 * @prop {(value: string) => void} [onChange] - text change event handler
 * @prop {string} [testId] - for testing purposes
 */

const PasswordInput = ({ ...props }: PasswordInputProps) => {
  const [showPassword, setShowPassword] = useState(false)

  return (
    <Input
      type={showPassword ? 'text' : 'password'}
      {...props}
      inputIcon={showPassword ? <Eye /> : <EyeSlash />}
      onIconClick={() => setShowPassword(show => !show)}
    />
  )
}

export default PasswordInput
