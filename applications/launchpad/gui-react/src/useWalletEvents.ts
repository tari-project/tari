import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

import { MinedTransactionsRepository } from './persistence/minedTransactionsRepository'

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
  Inbound = 'inbound',
  Outbound = 'outbound',
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
}

export const useWalletEvents = ({
  minedTransactionsRepository,
}: {
  minedTransactionsRepository: MinedTransactionsRepository
}) => {
  useEffect(() => {
    invoke('wallet_events')
  }, [])

  useEffect(() => {
    let unsubscribe

    const listenToEvents = async () => {
      unsubscribe = await listen(
        'wallet_event',
        ({
          event: _,
          payload,
        }: {
          event: string
          payload: WalletTransactionEvent
        }) => {
          minedTransactionsRepository.add(payload)
        },
      )
    }

    listenToEvents()

    return unsubscribe
  }, [])
}
