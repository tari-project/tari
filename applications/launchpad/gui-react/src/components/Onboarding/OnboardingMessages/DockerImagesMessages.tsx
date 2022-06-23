/* eslint-disable react/jsx-key */
import { useEffect, useState } from 'react'
import Text from '../../Text'
import t from '../../../locales'
import Button from '../../Button'
import { useAppDispatch } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { setExpertSwitchDisabled } from '../../../store/app'

/**
 * @TODO - temporary components for Docker Images Download - #309
 */

export const DownloadImagesMessage = ({
  onError,
  onSuccess,
}: {
  onError: (type: 'no_space_error' | 'server_error') => void
  onSuccess: () => void
}) => {
  const dispatch = useAppDispatch()
  const [status, setStatus] = useState<
    'inprogress' | 'no_space_error' | 'server_error' | 'success'
  >('inprogress')

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
      <Text>{status}</Text>
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
  const [status, setStatus] = useState<
    'notstarted' | 'inprogress' | 'no_space_error' | 'server_error' | 'success'
  >('notstarted')

  useEffect(() => {
    if (['no_space_error', 'server_error'].includes(status)) {
      onError(status as 'no_space_error' | 'server_error')
    }
  }, [status])

  return (
    <>
      <Text as='span' type='defaultMedium'>
        Error : {errorType}
      </Text>

      {status === 'notstarted' && (
        <Button onClick={() => setStatus('inprogress')}>
          <Text as='span' type='defaultUnder'>
            Try again
          </Text>
        </Button>
      )}

      {status === 'inprogress' && (
        <>
          <Text>In progress...</Text>
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
