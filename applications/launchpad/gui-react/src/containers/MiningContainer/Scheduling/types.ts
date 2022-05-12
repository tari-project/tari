import { MiningNodeType } from '../../../types/general'

type Time = {
  hours: number
  minutes: number
}

type Interval = {
  from: Time
  to: Time
}

export type Schedule = {
  enabled: boolean
  days: number[]
  interval: Interval
  type: MiningNodeType
}
