import { ReactNode } from 'react'

export interface CoinProps {
  amount: string
  unit: string
  suffixText?: string
  loading?: boolean
  icon?: ReactNode
}

export interface CoinsListProps {
  coins: CoinProps[]
  color?: string
  showSymbols?: boolean
}
