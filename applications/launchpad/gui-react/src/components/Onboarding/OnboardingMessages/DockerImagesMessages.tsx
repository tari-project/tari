/* eslint-disable react/jsx-key */
import { useEffect, useState } from 'react'
import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'
import { useAppDispatch, useAppSelector } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { setExpertSwitchDisabled } from '../../../store/app'
import { actions as dockerImagesActions } from '../../../store/dockerImages'
import { ActionStatusContainer, CtaButtonContainer, StatusRow } from './styles'
import Loading from '../../Loading'
import { selectDockerImages } from '../../../store/dockerImages/selectors'

type StatusType =
  | 'not_started'
  | 'in_progress'
  | 'no_space_error'
  | 'server_error'
  | 'success'

const Processing = () => (
  <ActionStatusContainer>
    <Loading loading />
    <Text type='defaultMedium'>{t.onboarding.status.processing}</Text>
  </ActionStatusContainer>
)

const Done = () => (
  <ActionStatusContainer>
    <StatusRow>
      <Text type='microMedium'>✅</Text>
      <Text type='defaultMedium'>{t.onboarding.status.done}</Text>
    </StatusRow>
  </ActionStatusContainer>
)

const Fail = () => (
  <ActionStatusContainer>
    <StatusRow>
      <Text type='microMedium'>❌</Text>
      <Text type='defaultMedium'>{t.onboarding.status.fail}</Text>
    </StatusRow>
  </ActionStatusContainer>
)

const Status = ({ status }: { status: StatusType }) => {
  switch (status) {
    case 'in_progress':
      return <Processing />
    case 'no_space_error':
    case 'server_error':
      return <Fail />
    case 'success':
      return <Done />
    default:
      return null
  }
}

export const DownloadImagesMessage = ({
  onError,
  onSuccess,
}: {
  onError: (type: 'no_space_error' | 'server_error') => void
  onSuccess: () => void
}) => {
  const dispatch = useAppDispatch()

  const dockerImages = useAppSelector(selectDockerImages)

  const [status, setStatus] = useState<StatusType>('in_progress')
  const [fetching, setFetching] = useState(false)
  const [accomplished, setAccomplished] = useState(false)

  useEffect(() => {
    dispatch(setExpertSwitchDisabled(false))
    setFetching(true)
    dispatch(dockerImagesActions.pullImages())
  }, [])

  useEffect(() => {
    const anyNotUpToDate = dockerImages.find(f => !f.updated)

    const anyError = dockerImages.find(f => Boolean(f.error))
    const anyInProgess = dockerImages.find(f => !f.updated && f.pending)

    if (fetching && !accomplished) {
      if (anyError) {
        if (anyError.error?.toLowerCase().includes('no space left')) {
          setStatus('no_space_error')
        } else {
          setStatus('server_error')
        }
        setFetching(false)
      } else if (!anyInProgess) {
        setStatus('success')
        setAccomplished(true)
        onSuccess()
        setFetching(false)
      }
    }

    if (!anyNotUpToDate && !accomplished) {
      setStatus('success')
      setAccomplished(true)
      onSuccess()
      setFetching(false)
      return
    }
  }, [dockerImages, fetching])

  useEffect(() => {
    if (['no_space_error', 'server_error'].includes(status)) {
      onError(status as 'no_space_error' | 'server_error')
    }
  }, [status])

  return (
    <>
      <Text as='span' type='defaultMedium'>
        {t.onboarding.dockerImages.message1.part1}
        <Button
          variant='button-in-text'
          onClick={() => dispatch(setExpertView('open'))}
        >
          <Text as='span' type='defaultUnder'>
            {t.onboarding.dockerImages.message1.part2}
          </Text>
        </Button>
      </Text>

      <Status status={status} />
    </>
  )
}

export const DownloadImagesErrorMessage = ({
  errorType,
  onError,
  onSuccess,
}: {
  errorType: 'no_space_error' | 'server_error'
  onError: (type: 'no_space_error' | 'server_error') => void
  onSuccess: () => void
}) => {
  const dispatch = useAppDispatch()

  const dockerImages = useAppSelector(selectDockerImages)

  const [status, setStatus] = useState<StatusType>('not_started')
  const [fetching, setFetching] = useState(false)
  const [accomplished, setAccomplished] = useState(false)

  useEffect(() => {
    if (['no_space_error', 'server_error'].includes(status)) {
      onError(status as 'no_space_error' | 'server_error')
    }

    if (status === 'in_progress' && !fetching) {
      dispatch(setExpertSwitchDisabled(false))
      setFetching(true)
      dispatch(dockerImagesActions.pullImages())
    }
  }, [status])

  useEffect(() => {
    if (fetching && !accomplished) {
      const anyError = dockerImages.find(f => Boolean(f.error))
      const anyInProgess = dockerImages.find(f => !f.updated && f.pending)

      if (anyError) {
        if (anyError.error?.toLowerCase().includes('no space left')) {
          setStatus('no_space_error')
        } else {
          setStatus('server_error')
        }
        setAccomplished(true)
        setFetching(false)
      } else if (!anyInProgess) {
        setStatus('success')
        setAccomplished(true)
        onSuccess()
        setFetching(false)
      }
    }
  }, [dockerImages, fetching])

  const text =
    errorType === 'no_space_error' ? (
      <Text as='span'>{t.onboarding.dockerImages.errors.noSpace}</Text>
    ) : (
      <>
        <Text as='span'>
          {t.onboarding.dockerImages.errors.serverError.part1}
        </Text>{' '}
        <Text as='span' type='defaultHeavy'>
          {t.onboarding.dockerImages.errors.serverError.part2}
        </Text>{' '}
        <Text as='span' type='defaultHeavy'>
          {t.onboarding.dockerImages.errors.serverError.part3}
        </Text>{' '}
        <Text as='span'>
          {t.onboarding.dockerImages.errors.serverError.part4}
        </Text>
      </>
    )

  return (
    <>
      <Text>{text}</Text>

      {status === 'not_started' && (
        <CtaButtonContainer>
          <Button onClick={() => setStatus('in_progress')}>
            <Text as='span'>{t.common.verbs.tryAgain}</Text>
          </Button>
        </CtaButtonContainer>
      )}

      <Status status={status} />
    </>
  )
}
