export interface CoinProps {
  amount: string
  unit: string
  suffixText?: string
  loading?: boolean
}

export interface CoinsListProps {
  coins: CoinProps[]
  color?: string
}
