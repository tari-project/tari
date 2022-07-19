import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'
import { listen } from '@tauri-apps/api/event'

import { AppDispatch } from './store'
import { actions } from './store/dockerImages'

export type DockerImagePullStatusEvent = {
  dockerImage: string
}

let isAlreadyInvoked = false

export const useDockerEvents = ({ dispatch }: { dispatch: AppDispatch }) => {
  useEffect(() => {
    if (isAlreadyInvoked) {
      return
    }

    let unsubscribe

    const listenToEvents = async () => {
      unsubscribe = await listen(
        'tari://pull_image_progress',
        async ({
          event: _,
          payload,
        }: {
          event: string
          payload: DockerImagePullStatusEvent
        }) => {
          dispatch(actions.setProgress(payload))
        },
      )
      isAlreadyInvoked = true
    }

    listenToEvents()

    invoke('wallet_events')

    return unsubscribe
  }, [])
}
