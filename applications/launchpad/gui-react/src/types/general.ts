import { CSSProperties } from 'react'
import { SpringValue } from 'react-spring'

export type CoinType = 'xtr' | ' xmr'

export type MiningNodeType = 'tari' | 'merged'

/**
 * Style types
 */
export type CSSWithSpring =
  | CSSProperties
  | Record<string, SpringValue<number>>
  | Record<string, SpringValue<string>>
