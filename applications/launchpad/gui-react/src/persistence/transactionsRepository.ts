import { useMemo } from 'react'
import groupby from 'lodash.groupby'

import {
  WalletTransactionEvent,
  TransactionEvent,
  TransactionDirection,
} from '../useWalletEvents'
import { Dictionary } from '../types/general'
import { useAppSelector } from '../store/hooks'
import { selectNetwork } from '../store/baseNode/selectors'
import { toT } from '../utils/Format'

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

export interface TransactionDBRecord {
  event: TransactionEvent
  id: string
  receivedAt: Date
  status: string
  direction: TransactionDirection
  amount: number
  message: string
  source: string
  destination: string
  isCoinbase: string
  network: string
}

export interface TransactionsRepository {
  addOrReplace: (transactionEvent: WalletTransactionEvent) => Promise<void>
  delete: (id: string) => Promise<void>
  getMinedXtr: (
    from: Date,
    to?: Date,
    resolution?: DataResolution,
  ) => Promise<Dictionary<MinedTariEntry>>
  hasDataBefore: (d: Date) => Promise<boolean>
  getLifelongMinedBalance: () => Promise<number>
  getMinedTransactionsDataSpan: () => Promise<{ from: Date; to: Date }>
  list: (limit: number, page?: number) => Promise<TransactionDBRecord[]>
  count: () => Promise<number>
  findById: (txId: string) => Promise<TransactionDBRecord | undefined>
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type WithAmount = { amount: number } & any
const toTariFromMicroTari = (result: WithAmount): WithAmount => ({
  ...result,
  amount: toT(result.amount),
})

const repositoryFactory: (
  network: string,
) => TransactionsRepository = network => ({
  addOrReplace: async event => {
    const db = await getDb()

    await db.execute(
      `INSERT OR REPLACE INTO
        transactions(event, id, receivedAt, status, direction, amount, message, source, destination, isCoinbase, network)
        values($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)`,
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
        network,
      ],
    )
  },
  delete: async id => {
    const db = await getDb()

    await db.execute('DELETE FROM transactions WHERE id = $1', [id])
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
        isCoinbase = 'true' AND
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
    const grouped = groupby(
      results.map(toTariFromMicroTari),
      grouping[resolution],
    )

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
        event = $1 AND
        isCoinbase = 'true'`,
      [TransactionEvent.Mined],
    )

    return results
      .map(toTariFromMicroTari)
      .reduce((accu, current) => accu + current.amount, 0)
  },
  getMinedTransactionsDataSpan: async () => {
    const db = await getDb()

    const resultsTo: {
      receivedAt: Date
    }[] = await db.select(
      `SELECT receivedAt FROM
        transactions
      WHERE
        event = $1 AND
        isCoinbase = 'true'
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
        event = $1 AND
        isCoinbase = 'true'
      ORDER BY receivedAt
      LIMIT 1`,
      [TransactionEvent.Mined],
    )

    return {
      from: new Date(resultsFrom[0]?.receivedAt) || new Date(),
      to: new Date(resultsTo[0]?.receivedAt) || new Date(),
    }
  },

  list: async (limit, page = 1) => {
    const db = await getDb()

    const results: TransactionDBRecord[] = await db.select(
      `SELECT * FROM
        transactions
      ORDER BY
        receivedAt DESC
      LIMIT $1
      OFFSET $2
     `,
      [limit, page * limit],
    )

    return results
  },

  count: async () => {
    const db = await getDb()

    /**
     * @TODO Using `SELECT COUNT(*)...` returns null.
     * The issue is already reported:
     * https://github.com/tauri-apps/tauri-plugin-sql/issues/121
     */
    const result: TransactionDBRecord[] = await db.select(
      'SELECT id FROM transactions',
    )

    if (!result) {
      return 0
    }

    return result.length
  },

  findById: async (txId: string) => {
    const db = await getDb()

    const result: TransactionDBRecord[] = await db.select(
      'SELECT * FROM transactions WHERE id = $1',
      [txId],
    )

    if (result?.length > 0) {
      return result[0]
    }

    return
  },
})

const useTransactionsRepository = () => {
  const network = useAppSelector(selectNetwork)

  const transactionsRepository = useMemo(
    () => repositoryFactory(network),
    [network],
  )

  return transactionsRepository
}

export default useTransactionsRepository
