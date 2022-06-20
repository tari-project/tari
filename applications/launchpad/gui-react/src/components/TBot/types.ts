import { CSSProperties } from 'styled-components'
export type TBotType = 'base' | 'hearts' | 'heartsMonero' | 'loading' | 'search'

/**
 * @typedef ShadowDefinition
 * @prop {string} [color] - color of the shadow
 * @prop {number} [spread] - box-shadow spread value
 * @prop {number} [blur] - box-shadow blur value
 */
export interface ShadowDefinition {
  color?: string
  spread?: number
  blur?: number
}

export interface CSSShadowDefinition {
  color: string
  spread: number
  blur: number
  size: number
}

export interface TBotProps {
  type?: TBotType
  style?: CSSProperties
  animate?: boolean
  shadow?: boolean | ShadowDefinition
  disableEnterAnimation?: boolean
}
