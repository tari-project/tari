import { ReactNode } from 'react'

/**
 * @typedef TextProps
 * @prop {'header' | 'subheader' | 'defaultHeavy' | 'defaultMedium' | 'defaultUnder' | 'smallHeavy' | 'smallMedium' | 'smallUnder' | 'microHeavy' | 'microRegular'  | 'microOblique' } [type] - text styles
 * @prop {ReactNode} children - text content to display
 * @prop {string} [color] - font color
 */

export interface TextProps {
  type?:
    | 'header'
    | 'subheader'
    | 'defaultHeavy'
    | 'defaultMedium'
    | 'defaultUnder'
    | 'smallHeavy'
    | 'smallMedium'
    | 'smallUnder'
    | 'microHeavy'
    | 'microRegular'
    | 'microOblique'
  children: ReactNode
  color?: string
}
