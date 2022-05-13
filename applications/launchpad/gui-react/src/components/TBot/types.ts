import { CSSProperties } from 'styled-components'
export type TBotType = 'base' | 'hearts' | 'heartsMonero' | 'loading' | 'search'

export interface TBotProps {
  type?: TBotType
  style?: CSSProperties
  animate?: boolean
}
