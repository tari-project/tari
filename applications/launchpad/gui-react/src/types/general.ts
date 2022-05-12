import { CSSProperties } from 'react'
import { SpringValue } from 'react-spring'

export type CoinType = 'xtr' | ' xmr'

export type MiningNodeType = 'tari' | 'merged'

export type Time = {
  hours: number
  minutes: number
}

export type Interval = {
  from: Time
  to: Time
}

export type Schedule = {
  id: string
  enabled: boolean
  days?: number[]
  date?: Date
  interval: Interval
  type: MiningNodeType[]
}

/**
 * Style types
 */
export type CSSWithSpring =
  | CSSProperties
  | Record<string, SpringValue<number>>
  | Record<string, SpringValue<string>>
