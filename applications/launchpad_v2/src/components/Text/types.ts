import { ReactNode, CSSProperties } from 'react'

/**
 * @typedef TextProps
 * @prop {'header' | 'subheader' | 'defaultHeavy' | 'defaultMedium' | 'defaultUnder' | 'smallHeavy' | 'smallMedium' | 'smallUnder' | 'microHeavy' | 'microRegular'  | 'microOblique' } [type] - text styles
 * @prop {ReactNode} children - text content to display
 * @prop {string} [color] - font color
 * @prop {CSSProperties} [style] - optional component styles
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
    | 'microMedium'
    | 'microRegular'
    | 'microOblique'
  children: ReactNode
  color?: string
  style?: CSSProperties
}
