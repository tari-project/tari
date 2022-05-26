import { MiningNodeType } from '../types/general'

export type StartStop = {
  start: Date
  stop: Date
  toMine: MiningNodeType
}
