import { CSSProperties } from 'react'
import { SpringValue } from 'react-spring'

export interface Dictionary<T> {
  [index: string]: T
}

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

export type ScheduleId = string
export type Schedule = {
  id: ScheduleId
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

export type ContainerName = string
export type ServiceRecipe = ContainerName[]
export type DockerImage = {
  imageName: string
  displayName: string
  dockerImage: string
  containerName: ContainerName
  updated: boolean
  pending?: boolean
  error?: string
  progress?: string
  status?: string
}
