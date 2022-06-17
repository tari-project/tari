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
  getLifelongMinedBalance: () => Promise<number>
  getMinedTransactionsDataSpan: () => Promise<{ from: Date; to: Date }>
}

const repositoryFactory: () => TransactionsRepository = () => ({
  add: async event => {
    const db = await getDb()

    await db.execute(
      `INSERT INTO
        transactions(event, id, receivedAt, status, direction, amount, message, source, destination, isCoinbase)
        values($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)`,
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
        event.is_coinbase,
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
      receivedAt: string
      amount: number
    }[] = await db.select(
      `SELECT
        receivedAt,
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
      [DataResolution.Daily]: ({ receivedAt }: { receivedAt: string }) =>
        receivedAt.substring(0, 10),
      [DataResolution.Monthly]: ({ receivedAt }: { receivedAt: string }) =>
        receivedAt.substring(0, 7),
      [DataResolution.Yearly]: ({ receivedAt }: { receivedAt: string }) =>
        receivedAt.substring(0, 4),
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
  getLifelongMinedBalance: async () => {
    const db = await getDb()

    const results: {
      amount: number
    }[] = await db.select(
      `SELECT amount FROM
        transactions
      WHERE
        event = $1`,
      [TransactionEvent.Mined],
    )

    return results.reduce((accu, current) => accu + current.amount, 0)
  },
  getMinedTransactionsDataSpan: async () => {
    const db = await getDb()

    const resultsTo: {
      receivedAt: Date
    }[] = await db.select(
      `SELECT receivedAt FROM
        transactions
      WHERE
        event = $1
      ORDER BY receivedAt DESC
      LIMIT 1`,
      [TransactionEvent.Mined],
    )

    const resultsFrom: {
      receivedAt: Date
    }[] = await db.select(
      `SELECT receivedAt FROM
        transactions
      WHERE
        event = $1
      ORDER BY receivedAt
      LIMIT 1`,
      [TransactionEvent.Mined],
    )

    return {
      from: new Date(resultsFrom[0]?.receivedAt) || new Date(),
      to: new Date(resultsTo[0]?.receivedAt) || new Date(),
    }
  },
})

export default repositoryFactory
