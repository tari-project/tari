import { WalletTransactionEvent } from '../useWalletEvents'

enum Resolution {
  Daily = 'daily',
  Monthly = 'monthly',
  Yearly = 'yearly',
}

export interface WalletTransactionEntry {
  event: string
  id: string
  receivedAt: Date
  status: string
  direction: string
  amount: number
  message: string
  source: string
  destination: string
}

export interface TransactionsRepository {
  add: (transactionEvent: WalletTransactionEvent) => Promise<void>
  get: (
    from: Date,
    to: Date,
    resolution: Resolution,
  ) => Promise<WalletTransactionEntry[]>
}

const repositoryFactory: () => TransactionsRepository = () => ({
  add: async event => {
    console.debug('adding transaction event', event)
  },
  get: async (from, to, resolution = Resolution.Daily) => {
    console.debug('get transaction', from, to, resolution)

    return [] as WalletTransactionEntry[]
  },
})

export default repositoryFactory
