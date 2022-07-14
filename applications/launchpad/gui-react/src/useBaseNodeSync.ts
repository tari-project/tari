import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

export type SyncType = 'Startup' | 'Header' | 'Block'

export type BaseNodeSyncProgress = {
  estimatedTimeSec: number
  syncType: SyncType
  headerProgress: number
  blockProgress: number
  totalBlocks: number
  done: boolean
}

export type SyncProgress = {
  syncType?: SyncType
  progress?: number
  remainingTime?: number
  finished?: boolean
}

const wait = (ms: number) => {
  return new Promise(resolve => setTimeout(resolve, ms))
}

let isAlreadyInvoked = false

/**
 * Listen to the Base Node sync progress
 */
export const useBaseNodeSync = (started: boolean) => {
  const [progress, setProgress] = useState<SyncProgress>({
    progress: undefined,
    remainingTime: undefined,
    syncType: undefined,
  })

  const invokeWithDelay = async () => {
    /**
     * @TODO if we do not wait, the backend has an issue base_node_gprc
     * bc base node is not running yet.
     * Should this be handled by the backend?
     * Or frontend can check the status_check/health first?
     * Because waiting arbirtary 10sec is not a good idea.
     */
    await wait(10000)

    invoke('base_node_sync_progress')
  }

  useEffect(() => {
    if (isAlreadyInvoked) {
      return
    }

    let unsubscribe

    const listenToEvents = async () => {
      unsubscribe = await listen(
        'tari://onboarding_progress',
        async ({
          event: _,
          payload,
        }: {
          event: string
          payload: BaseNodeSyncProgress
        }) => {
          try {
            setProgress({
              progress:
                payload.syncType === 'Block'
                  ? payload.blockProgress
                  : payload.headerProgress,
              remainingTime: payload.estimatedTimeSec,
              syncType: payload.syncType,
              finished: payload.done,
            })
          } catch (_) {
            setProgress({
              progress: undefined,
              remainingTime: undefined,
              syncType: undefined,
              finished: undefined,
            })
          }
        },
      )

      isAlreadyInvoked = true
    }

    if (started) {
      listenToEvents()
      invokeWithDelay()
    }

    return unsubscribe
  }, [started])

  return progress
}
