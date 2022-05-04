import { ReactNode, CSSProperties } from 'react'
import { AnimatedComponent, SpringValue } from 'react-spring'

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
  style?: CSSProperties | Record<string, SpringValue<string>>
  as?:
    | 'h1'
    | 'h2'
    | 'h3'
    | 'h4'
    | 'h4'
    | 'h5'
    | 'h6'
    | 'p'
    | 'span'
    | 'label'
    | AnimatedComponent<
        'h1' | 'h2' | 'h3' | 'h4' | 'h4' | 'h5' | 'h6' | 'p' | 'span' | 'label'
      >
  testId?: string
  className?: string
}
