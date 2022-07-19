import { useEffect } from 'react'
import { invoke } from '@tauri-apps/api/tauri'

import { DockerImage, ServiceRecipe } from './types/general'

import { AppDispatch } from './store'
import { useAppSelector } from './store/hooks'
import { selectServiceSettings } from './store/settings/selectors'
import { actions as dockerImagesActions } from './store/dockerImages'

import AppConfig from './config/app'

export type DockerImagePullStatusEvent = {
  dockerImage: string
}

let isAlreadyRunning = false

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const checkImages = async (serviceSettings: any, dispatch: AppDispatch) => {
  try {
    isAlreadyRunning = true

    const result = await invoke<{
      imageInfo: DockerImage[]
      serviceRecipes: ServiceRecipe[]
    }>('image_info', {
      settings: serviceSettings,
    })

    const outdated = result.imageInfo.filter(i => !i.updated)
    outdated.map(i => {
      dispatch(dockerImagesActions.pushToTBotQueue(i))
    })
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err)
  }
}

export const useCheckDockerImages = ({
  dispatch,
}: {
  dispatch: AppDispatch
}) => {
  const serviceSettings = useAppSelector(selectServiceSettings)

  useEffect(() => {
    if (isAlreadyRunning) {
      return
    }

    setTimeout(() => {
      checkImages(serviceSettings, dispatch)
    }, 10000)

    const interval = setInterval(() => {
      checkImages(serviceSettings, dispatch)
    }, AppConfig.dockerNewImagesCheckInterval)

    return () => clearInterval(interval)
  }, [])
}
