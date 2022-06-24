import { ReactNode, CSSProperties } from 'react'
import { AnimatedComponent, SpringValue } from 'react-spring'

/**
 * @typedef TextProps
 * @prop {'header' | 'subheader' | 'defaultHeavy' | 'defaultMedium' | 'defaultUnder' | 'smallHeavy' | 'smallMedium' | 'smallUnder' | 'microHeavy' | 'microRegular'  | 'microOblique' } [type] - text styles
 * @prop {ReactNode} children - text content to display
 * @prop {string} [color] - font color
 * @prop {CSSProperties} [style] - optional component styles
 * @prop {'h1' | 'h2' | 'h3' | 'h4' | 'h4' | 'h5' | 'h6' | 'p' | 'span' | 'label' | AnimatedComponent<'h1' | 'h2' | 'h3' | 'h4' | 'h4' | 'h5' | 'h6' | 'p' | 'span' | 'label'> } [as] - prop controlling what component to use for text
 */

export type TextType =
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

export interface TextProps {
  type?: TextType
  children: ReactNode
  color?: string
  style?:
    | CSSProperties
    | Record<string, SpringValue<string> | SpringValue<number>>
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
