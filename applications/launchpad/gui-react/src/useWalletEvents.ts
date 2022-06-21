import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

import { TransactionsRepository } from './persistence/transactionsRepository'

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

let isSubscribedSuperstate = false
export const useWalletEvents = ({
  transactionsRepository,
}: {
  transactionsRepository: TransactionsRepository
}) => {
  useEffect(() => {
    if (isSubscribedSuperstate) {
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
          transactionsRepository.add(payload)
        },
      )
      isSubscribedSuperstate = true
    }

    listenToEvents()

    invoke('wallet_events')

    return unsubscribe
  }, [])
}
