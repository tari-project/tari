import { useEffect, useMemo, useState } from 'react'

import Text from '../../../Text'

import MessagesConfig from '../../../../config/helpMessagesConfig'
import t from '../../../../locales'

import {
  selectDockerImages,
  selectDockerTBotQueue,
} from '../../../../store/dockerImages/selectors'
import { useAppDispatch, useAppSelector } from '../../../../store/hooks'
import { tbotactions } from '../../../../store/tbot'
import { actions as dockerImagesActions } from '../../../../store/dockerImages'

import {
  ButtonsContainer,
  DockerDwnlTag,
  DockerDwnlTagContainer,
  DockerDwnlInnerTag,
  ProgressContainer,
} from './styles'
import Button from '../../../Button'
import { selectExpertView, selectTheme } from '../../../../store/app/selectors'

export const NewDockerImageToDownload = () => {
  const dockerImages = useAppSelector(selectDockerTBotQueue)
  const expertView = useAppSelector(selectExpertView)
  const currentTheme = useAppSelector(selectTheme)

  const dockerImage = useMemo(() => dockerImages[0], [])

  return (
    <div>
      <DockerDwnlTagContainer>
        <DockerDwnlTag
          $dark={currentTheme === 'dark' || expertView !== 'hidden'}
        >
          <Text type='microMedium' as='span' style={{ flex: 1 }}>
            {dockerImage?.displayName || ''}
          </Text>
          <DockerDwnlInnerTag>
            <Text type='microMedium' as='span'>
              {t.docker.newerVersion}
            </Text>
          </DockerDwnlInnerTag>
        </DockerDwnlTag>
      </DockerDwnlTagContainer>
      <Text type='defaultMedium'>
        <Text type='defaultHeavy' as='span'>
          {t.docker.tBot.newVersionAvailable.part1}
        </Text>{' '}
        {t.docker.tBot.newVersionAvailable.part2}
      </Text>
    </div>
  )
}

export const DownloadDockerImage = () => {
  const dispatch = useAppDispatch()

  const dockerImagesQueue = useAppSelector(selectDockerTBotQueue)
  const dockerImages = useAppSelector(selectDockerImages)

  const [status, setStatus] = useState<
    'not_started' | 'processing' | 'done' | 'error'
  >('not_started')
  const [progress, setProgress] = useState('')

  const dockerImage = useMemo(() => dockerImagesQueue[0], [])

  useEffect(() => {
    const foundImage = dockerImages.find(
      i => i.containerName === dockerImage.containerName,
    )

    if (foundImage) {
      setProgress(foundImage.status || '')

      if (!foundImage.pending && foundImage.updated) {
        setStatus('done')
        dispatch(tbotactions.push(MessagesConfig.DockerImageDownloadSuccess))
      } else if (foundImage.error) {
        setStatus('error')
        dispatch(tbotactions.push(MessagesConfig.DockerImageDownloadError))
      }
    }
  }, [dockerImages])

  const dismiss = () => {
    dispatch(tbotactions.close())
    dispatch(
      dockerImagesActions.removeFromTBotQueue({
        image: dockerImage,
        dismiss: true,
      }),
    )
  }

  const download = () => {
    setStatus('processing')

    dispatch(
      dockerImagesActions.pullImage({
        dockerImage: dockerImage.containerName,
      }),
    )
  }

  return (
    <div>
      <Text>{t.docker.tBot.downloadStepMessage}</Text>
      {status === 'not_started' && (
        <ButtonsContainer>
          <Button variant='secondary' onClick={dismiss}>
            {t.common.verbs.dismiss}
          </Button>
          <Button onClick={download}>{t.docker.pullNewerImage}</Button>
        </ButtonsContainer>
      )}

      {status === 'processing' && (
        <ProgressContainer>
          <Text type='microMedium' style={{ width: '100%' }}>
            {progress}
          </Text>
        </ProgressContainer>
      )}

      {status === 'done' && (
        <ProgressContainer>
          <Text type='microMedium'>✅</Text>
        </ProgressContainer>
      )}

      {status === 'error' && (
        <ProgressContainer>
          <Text type='microMedium'>❌</Text>
        </ProgressContainer>
      )}
    </div>
  )
}

export const DockerImageDownloadSuccess = () => {
  const dispatch = useAppDispatch()

  const dockerImagesQueue = useAppSelector(selectDockerTBotQueue)

  const dockerImage = useMemo(() => dockerImagesQueue[0], [])

  useEffect(() => {
    dispatch(
      dockerImagesActions.removeFromTBotQueue({
        image: dockerImage,
        dismiss: false,
      }),
    )
  }, [dockerImage])

  return (
    <div>
      <ButtonsContainer>
        <Text>
          ✅{' '}
          <Text type='defaultHeavy' as='span'>
            {t.docker.tBot.downloadSuccess.part1}
          </Text>{' '}
          {t.docker.tBot.downloadSuccess.part2}
        </Text>
      </ButtonsContainer>
    </div>
  )
}

export const DockerImageDownloadError = () => {
  const dispatch = useAppDispatch()

  const dockerImagesQueue = useAppSelector(selectDockerTBotQueue)

  const dockerImage = useMemo(() => dockerImagesQueue[0], [])

  useEffect(() => {
    dispatch(
      dockerImagesActions.removeFromTBotQueue({
        image: dockerImage,
        dismiss: false,
      }),
    )
  }, [dockerImage])

  return (
    <div>
      <ButtonsContainer>
        <Text>
          ❌{' '}
          <Text type='defaultHeavy' as='span'>
            {t.docker.tBot.downloadError.part1}
          </Text>{' '}
          {t.docker.tBot.downloadError.part2}
        </Text>
      </ButtonsContainer>
    </div>
  )
}
