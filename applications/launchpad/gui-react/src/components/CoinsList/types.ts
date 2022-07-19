import { ReactNode } from 'react'

export interface CoinProps {
  amount: string | number
  unit: string
  suffixText?: string
  loading?: boolean
  icon?: ReactNode
}

export interface CoinsListProps {
  coins: CoinProps[]
  inline?: boolean
  small?: boolean
  color?: string
  unitsColor?: string
  showSymbols?: boolean
}
