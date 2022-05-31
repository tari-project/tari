import { MiningNodeType, ScheduleId } from '../types/general'

export type StartStop = {
  start: Date
  stop: Date
  toMine: MiningNodeType
  scheduleId: ScheduleId
}
