import { ReactNode } from 'react'

export interface SelectInternalProps {
  disabled?: boolean
  inverted?: boolean
  children?: ReactNode
  open?: boolean
  fullWidth?: boolean
}

/**
 * @typedef {Object} Option
 * @property {string | number} value - value of the option
 * @property {string} label - label shown in option
 * @property {string} key - key to be used in react map
 */
export type Option = {
  value: string | number
  label: string
  key: string
}
