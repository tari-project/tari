import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

import { AppDispatch } from './store'
import { actions } from './store/containers'

enum SystemEventType {
  Container = 'container',
}

let isAlreadyInvoked = false

export const useSystemEvents = ({ dispatch }: { dispatch: AppDispatch }) => {
  useEffect(() => {
    if (isAlreadyInvoked) {
      return
    }

    let unsubscribe

    const listenToSystemEvents = async () => {
      unsubscribe = await listen(
        'tari://docker-system-event',
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        (event: any) => {
          if (event.payload.Type === SystemEventType.Container) {
            const containerId = event.payload.Actor.ID
            const action = event.payload.Action
            const image = event.payload.Actor.Attributes.image
            if (!image.startsWith('quay.io/tarilabs')) {
              return
            }
            let exitCode: number | undefined

            try {
              exitCode = event.payload.Actor?.Attributes?.exitCode
                ? Number(event.payload.Actor?.Attributes?.exitCode)
                : undefined
            } catch (_) {
              // Exit code is not a number
            }

            dispatch(actions.updateStatus({ containerId, action, exitCode }))

            return
          }
        },
      )
      isAlreadyInvoked = true
    }

    listenToSystemEvents()

    invoke('events')

    return unsubscribe
  }, [])
}
