import { CSSProperties } from 'styled-components'
export type TBotType = 'base' | 'hearts' | 'heartsMonero' | 'loading' | 'radar'

export interface TBotProps {
  type: TBotType
  size?: number
  style?: CSSProperties
}
