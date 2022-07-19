import { listen, UnlistenFn } from '@tauri-apps/api/event'
import { useEffect } from 'react'
import { AppDispatch } from '../store'
import { actions as dockerImagesActions } from '../store/dockerImages'
import { selectDockerImages } from '../store/dockerImages/selectors'
import { useAppSelector } from '../store/hooks'

let listenerActive = false

export interface ImagePullProgress {
  current: null
  error?: string
  id: string
  image_name: string
  progress: string
  status: string
  total: number
}

export const useDockerImageDownloadListener = ({
  dispatch,
}: {
  dispatch: AppDispatch
}) => {
  const dockerImages = useAppSelector(selectDockerImages)

  const anyDownloading = dockerImages.find(d => d.pending)

  useEffect(() => {
    let unlisten: UnlistenFn | undefined = undefined

    const listenToDownload = async () => {
      unlisten = await listen(
        'tari://image_pull_progress',
        async ({ payload }: { event: string; payload: ImagePullProgress }) => {
          dispatch(
            dockerImagesActions.setProgress({
              dockerImage: payload.image_name,
              progress: payload.progress,
              status: payload.status,
              error: payload.error,
            }),
          )
        },
      )
    }

    if (anyDownloading && !listenerActive) {
      listenToDownload()
      listenerActive = true
    }

    return unlisten
  }, [anyDownloading])
}
