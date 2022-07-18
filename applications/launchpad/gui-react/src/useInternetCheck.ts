import { invoke } from '@tauri-apps/api'
import { useEffect } from 'react'
import MessagesConfig from './config/helpMessagesConfig'
import { AppDispatch } from './store'
import { tbotactions } from './store/tbot'

export const useInternetCheck = ({ dispatch }: { dispatch: AppDispatch }) => {
  useEffect(() => {
    const interval = setInterval(async () => {
      try {
        const isOnline = await invoke('check_internet_connection')

        if (!isOnline) {
          dispatch(tbotactions.push(MessagesConfig.OnlineCheck))
        }
      } catch (_) {
        // Do not propagate further
      }
    }, 10000)

    return () => clearInterval(interval)
  }, [])
}
