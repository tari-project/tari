import { MiningNodeType } from '../../../types/general'

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
