import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

import { AppDispatch } from './store'
import { actions } from './store/containers'

enum SystemEventType {
  Container = 'container',
}

export const useSystemEvents = ({ dispatch }: { dispatch: AppDispatch }) => {
  useEffect(() => {
    invoke('events')
  }, [])

  useEffect(() => {
    let unsubscribe

    const listenToSystemEvents = async () => {
      const systemEvents = new Map<string, number>()
      const unlisten = await listen(
        'tari://docker-system-event',
        (event: any) => {
          if (event.payload.Type === SystemEventType.Container) {
            const containerId = event.payload.Actor.ID
            const action = event.payload.Action
            const image = event.payload.Actor.Attributes.image
            if (!image.startsWith('quay.io/tarilabs')) {
              return
            }

            console.log(`ACTION ${action} - ${containerId} - ${image}`)
            console.log(event)

            dispatch(actions.updateStatus({ containerId, action }))

            return
          }
        },
      )

      unsubscribe = () => {
        console.log(JSON.stringify(systemEvents, null, 2))
        unlisten()
      }
    }

    listenToSystemEvents()

    return unsubscribe
  }, [])
}
