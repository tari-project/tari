import groupby from 'lodash.groupby'

import { WalletTransactionEvent, TransactionEvent } from '../useWalletEvents'
import { Dictionary } from '../types/general'

import getDb from './db'

export enum DataResolution {
  Daily = 'daily',
  Monthly = 'monthly',
  Yearly = 'yearly',
}

export interface MinedTariEntry {
  when: string
  xtr: number
}

export interface TransactionsRepository {
  add: (transactionEvent: WalletTransactionEvent) => Promise<void>
  getMinedXtr: (
    from: Date,
    to?: Date,
    resolution?: DataResolution,
  ) => Promise<Dictionary<MinedTariEntry>>
  hasDataBefore: (d: Date) => Promise<boolean>
}

const repositoryFactory: () => TransactionsRepository = () => ({
  add: async event => {
    const db = await getDb()

    const now = new Date()
    const year = `${now.getFullYear()}`
    const month = `${year}-${now.getMonth().toString().padStart(2, '0')}`
    const day = `${month}-${now.getDate().toString().padStart(2, '0')}`
    await db.execute(
      'INSERT INTO transactions(event, id, receivedAt, year, month, day, status, direction, amount, message, source, destination), values($1, $2, $3, $4, $5, $6, $7, $8, $9)',
      [
        event.event,
        event.tx_id,
        now,
        year,
        month,
        day,
        event.status,
        event.direction,
        event.amount,
        event.message,
        event.source_pk,
        event.dest_pk,
      ],
    )
  },
  getMinedXtr: async (
    from,
    to = new Date(),
    resolution = DataResolution.Daily,
  ) => {
    const db = await getDb()

    const results: {
      year: string
      month: string
      day: string
      amount: number
    }[] = await db.select(
      `SELECT
        year,
        month,
        day,
        amount
      FROM
        transactions
      WHERE
        event = $1 AND
        receivedAt >= $2 AND
        receivedAt <= $3`,
      [TransactionEvent.Mined, from, to],
    )

    const grouping = {
      [DataResolution.Daily]: 'day',
      [DataResolution.Monthly]: 'month',
      [DataResolution.Yearly]: 'year',
    }
    const grouped = groupby(results, grouping[resolution])

    return Object.fromEntries(
      Object.entries(grouped).map(([when, entries]) => [
        when,
        {
          when,
          xtr: entries.reduce((accu, current) => accu + current.amount, 0),
        },
      ]),
    )
  },
  hasDataBefore: async date => {
    const db = await getDb()

    const results: { id: string }[] = await db.select(
      'SELECT id FROM transactions WHERE receivedAt < $1 LIMIT 1',
      [date],
    )

    return Boolean(results.length)
  },
})

export default repositoryFactory
