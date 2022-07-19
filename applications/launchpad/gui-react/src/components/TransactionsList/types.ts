import { TransactionDBRecord } from '../../persistence/transactionsRepository'

export interface TransactionsListProps {
  records: TransactionDBRecord[]
  inverted: boolean
}
