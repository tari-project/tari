/* eslint-disable react/jsx-key */
import { useEffect, useState } from 'react'
import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'
import { useAppDispatch } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { setExpertSwitchDisabled } from '../../../store/app'
import { ActionStatusContainer, CtaButtonContainer, StatusRow } from './styles'
import Loading from '../../Loading'

/**
 * @TODO - temporary components for Docker Images Download - #309
 */

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
  const [status, setStatus] = useState<StatusType>('in_progress')

  useEffect(() => {
    dispatch(setExpertSwitchDisabled(false))
  }, [])

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

      <Button
        onClick={() => {
          setStatus('success')
          onSuccess()
        }}
      >
        Success
      </Button>
      <Button
        onClick={() => {
          setStatus('server_error')
        }}
      >
        Error 1 - server error
      </Button>
      <Button
        onClick={() => {
          setStatus('no_space_error')
        }}
      >
        Error 2 - no space
      </Button>
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
  const [status, setStatus] = useState<StatusType>('not_started')

  useEffect(() => {
    if (['no_space_error', 'server_error'].includes(status)) {
      onError(status as 'no_space_error' | 'server_error')
    }
  }, [status])

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
          (#NUMBER)
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

      {status === 'in_progress' && (
        <>
          <Button
            onClick={() => {
              setStatus('success')
              onSuccess()
            }}
          >
            Success
          </Button>
          <Button
            onClick={() => {
              setStatus('server_error')
            }}
          >
            Error 1 - server error
          </Button>
          <Button
            onClick={() => {
              setStatus('no_space_error')
            }}
          >
            Error 2 - no space
          </Button>
        </>
      )}
    </>
  )
}
