import { WalletTransactionEvent, TransactionEvent } from '../useWalletEvents'
import getDb from './db'

enum Resolution {
  Daily = 'daily',
  Monthly = 'monthly',
  Yearly = 'yearly',
}

export interface MinedAmountEntry {
  when: string
  amount: number
}

export interface TransactionsRepository {
  add: (transactionEvent: WalletTransactionEvent) => Promise<void>
  getMinedXtr: (
    from: Date,
    to: Date,
    resolution: Resolution,
  ) => Promise<MinedAmountEntry[]>
}

const repositoryFactory: () => TransactionsRepository = () => ({
  add: async event => {
    const db = await getDb()

    await db.execute(
      'INSERT INTO transactions(event, id, receivedAt, status, direction, amount, message, source, destination), values($1, $2, $3, $4, $5, $6, $7, $8, $9)',
      [
        event.event,
        event.tx_id,
        new Date(),
        event.status,
        event.direction,
        event.amount,
        event.message,
        event.source_pk,
        event.dest_pk,
      ],
    )
  },
  getMinedXtr: async (from, to = new Date(), resolution = Resolution.Daily) => {
    const db = await getDb()

    const amountQueries = {
      [Resolution.Daily]: 'date("receivedAt")',
      // eslint-disable-next-line quotes
      [Resolution.Monthly]: `strftime('%Y-%m')`,
      // eslint-disable-next-line quotes
      [Resolution.Yearly]: `strftime('%Y')`,
    }

    const results: MinedAmountEntry[] = await db.select(
      `SELECT
        ${amountQueries[resolution]} as when,
        sum(amount) as amount
      FROM
        transactions
      WHERE
        event = $1 AND
        receivedAt >= $2 AND
        receivedAt <= $3
      GROUP BY when`,
      [TransactionEvent.Mined, from, to],
    )

    return results
  },
})

export default repositoryFactory
