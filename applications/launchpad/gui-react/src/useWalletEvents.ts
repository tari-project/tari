import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

import { AppDispatch } from './store'

export type WalletTransactionEvent = {
  event: string
  tx_id: string
  source_pk: string
  dest_pk: string
  status: string
  direction: string
  amount: number
  message: string
}

export const useWalletEvents = ({ dispatch }: { dispatch: AppDispatch }) => {
  useEffect(() => {
    invoke('wallet_events')
  }, [])

  useEffect(() => {
    let unsubscribe

    const listenToEvents = async () => {
      unsubscribe = await listen(
        'wallet_event',
        ({
          event,
          payload,
        }: {
          event: string
          payload: WalletTransactionEvent
        }) => {
          console.debug({ event, payload })
        },
      )
    }

    listenToEvents()

    return unsubscribe
  }, [])
}
