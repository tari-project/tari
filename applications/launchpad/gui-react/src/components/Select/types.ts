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
export interface Option {
  value: string | number
  label: string
  key: string
}

/**
 * @typedef {Object} SelectStylesOverrideProps
 * @property {Object} [icon] - down arrow styles override
 * @property {string} [color] - color of the icon
 * @property {Object} [value] - overrides of the box showing selected value
 * @property {string} [color] - text color of the value
 * @property {string} [backgroundColor] - background color
 * @property {(open?: boolean) => string} [borderColor] - allows to set different border color for opened and closed state
 * @property {Object} [label] - label styles override
 * @property {string} [color] - color of the label
 */
export type SelectStylesOverrideProps = {
  icon?: {
    color?: string
  }
  value?: {
    color?: string
    backgroundColor?: string
    borderColor?: (open?: boolean) => string
  }
  label?: {
    color?: string
  }
}
