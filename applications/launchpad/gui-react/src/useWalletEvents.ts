import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

import { TransactionsRepository } from './persistence/transactionsRepository'
import { AppDispatch } from './store'
import { actions as miningActions } from './store/mining'
import { toT } from './utils/Format'

export enum TransactionEvent {
  Received = 'received',
  Sent = 'sent',
  Queued = 'queued',
  Confirmation = 'confirmation',
  Mined = 'mined',
  Cancelled = 'cancelled',
  NewBlockMined = 'new_block_mined',
}

export enum TransactionDirection {
  Inbound = 'Inbound',
  Outbound = 'Outbound',
}

export type WalletTransactionEvent = {
  event: TransactionEvent
  tx_id: string
  source_pk: string
  dest_pk: string
  status: string
  direction: TransactionDirection
  amount: number
  message: string
  is_coinbase: boolean
}

let isAlreadyInvoked = false

export const useWalletEvents = ({
  dispatch,
  transactionsRepository,
}: {
  dispatch: AppDispatch
  transactionsRepository: TransactionsRepository
}) => {
  useEffect(() => {
    if (isAlreadyInvoked) {
      return
    }

    let unsubscribe

    const listenToEvents = async () => {
      unsubscribe = await listen(
        'tari://wallet_event',
        async ({
          event: _,
          payload,
        }: {
          event: string
          payload: WalletTransactionEvent
        }) => {
          /**
           * @TODO add 'if' statement that will filter uninteresting events.
           * For addMindexTx, we need only 'mined' with 'is_coinbase === true' (?)
           * (waiting for confirmation)
           */
          // if (payload.is_coinbase && status.toLowerCase() === 'mined confirmed') {
          dispatch(
            miningActions.addMinedTx({
              amount: toT(payload.amount),
              node: 'tari',
              txId: payload.tx_id,
            }),
          )
          transactionsRepository.add(payload)
          // }
        },
      )
      isAlreadyInvoked = true
    }

    listenToEvents()

    invoke('wallet_events')

    return unsubscribe
  }, [])
}
