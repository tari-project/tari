import { CoinType } from '../../../types/general'

export type MiningStatisticsInterval = 'monthly' | 'yearly' | 'all'

export type AccountData = {
  balance: {
    value: number
    currency: CoinType
  }
  delta: {
    percentage: number
    interval: MiningStatisticsInterval
  }
}[]
