import { useState, useEffect, useRef } from 'react'
import { hideSplashscreen } from '../../splashscreen'
import { isDockerInstalled } from '../../commands'

import Button from '../../components/Button'
import TBotPrompt from '../../components/TBot/TBotPrompt'
import { TBotMessage } from '../../components/TBot/TBotPrompt/types'
import {
  OnboardingMessagesIntro,
  OnboardingMessagesDockerInstall,
  OnboardingMessagesDockerInstallAfter,
  DownloadImagesMessage,
  DownloadImagesErrorMessage,
  BlockchainSyncStep,
} from '../../config/onboardingMessagesConfig'
import {
  setExpertSwitchDisabled,
  setExpertView,
  setOnboardingComplete,
} from '../../store/app'
import { selectOnboardingCheckpoint } from '../../store/app/selectors'
import { OnboardingCheckpoints } from '../../store/app/types'
import { useAppDispatch, useAppSelector } from '../../store/hooks'
import t from '../../locales'
import { StyledOnboardingContainer } from './styles'
import Text from '../../components/Text'

const OnboardingContainer = () => {
  const dispatch = useAppDispatch()

  const messagesRef = useRef<TBotMessage[]>()

  const lastOnboardingCheckpoint = useAppSelector(selectOnboardingCheckpoint)

  const [messages, setMessages] = useState(OnboardingMessagesIntro)
  const [dockerInstalled, setDockerInstalled] = useState<boolean | undefined>(
    undefined,
  )
  const [current, setCurrent] = useState(0)
  const [tBotIndex, setTBotIndex] = useState(1)

  useEffect(() => {
    dispatch(setExpertView('hidden'))
    dispatch(setExpertSwitchDisabled(true))
  }, [])

  messagesRef.current = messages

  const checkDocker = async () => {
    let isDocker
    try {
      isDocker = await isDockerInstalled()

      setDockerInstalled(isDocker)
      hideSplashscreen()
    } catch (err) {
      isDocker = false
      setDockerInstalled(false)
    } finally {
      hideSplashscreen()
    }

    if (
      isDocker &&
      lastOnboardingCheckpoint === OnboardingCheckpoints.DOCKER_INSTALL
    ) {
      setMessages(
        OnboardingMessagesDockerInstallAfter.concat([
          {
            content: () => (
              <DownloadImagesMessage
                key='dim'
                onError={onDockerImageDownloadError}
                onSuccess={onImagesDowloadSuccess}
              />
            ),
            barFill: 0.625,
            noSkip: true,
          },
        ]),
      )
    }
  }

  useEffect(() => {
    checkDocker()
  }, [])

  const pushMessages = (msgs: TBotMessage[]) => {
    if (!messagesRef.current) {
      return
    }
    const newMsgs = messagesRef.current.concat(msgs)
    setMessages(newMsgs)
  }

  const onImagesDowloadSuccess = () => {
    pushMessages([
      {
        content: <BlockchainSyncStep pushMessages={pushMessages} />,
        barFill: 0.875,
        wait: 500,
        noSkip: true,
      },
    ])
  }

  const onDockerInstallDone = () => {
    pushMessages(
      OnboardingMessagesDockerInstallAfter.concat([
        {
          content: () => (
            <DownloadImagesMessage
              key='dim'
              onError={onDockerImageDownloadError}
              onSuccess={onImagesDowloadSuccess}
            />
          ),
          barFill: 0.625,
          noSkip: true,
        },
      ]),
    )
  }

  const onDockerImageDownloadError = (
    type: 'no_space_error' | 'server_error',
  ) => {
    pushMessages([
      {
        content: () => (
          <DownloadImagesErrorMessage
            key={`dim-${messages.length}`}
            errorType={type}
            onError={onDockerImageDownloadError}
            onSuccess={onImagesDowloadSuccess}
          />
        ),
        barFill: 0.625,
        noSkip: true,
        wait: 100,
      },
    ])
  }

  /** IS DOCKER INSTALLED */
  useEffect(() => {
    // Do not push Docker related messages to queue until the intro is done
    if (tBotIndex !== 4 || (dockerInstalled && lastOnboardingCheckpoint)) {
      return
    }

    if (!dockerInstalled) {
      pushMessages(OnboardingMessagesDockerInstall(onDockerInstallDone))
    } else if (dockerInstalled === true) {
      pushMessages([
        {
          content: () => (
            <DownloadImagesMessage
              key={`dim-${messages.length}`}
              onError={onDockerImageDownloadError}
              onSuccess={onImagesDowloadSuccess}
            />
          ),
          barFill: 0.625,
          noSkip: true,
        },
      ])
    }
  }, [dockerInstalled, tBotIndex])

  const onMessageRender = (index: number) => {
    setTBotIndex(index)
  }

  const onSkip = () => {
    setCurrent(messages.length)
  }

  return (
    <StyledOnboardingContainer>
      <Button
        variant='secondary'
        onClick={() => dispatch(setOnboardingComplete(true))}
        style={{
          position: 'absolute',
          bottom: 40,
          left: 40,
        }}
      >
        <Text type='smallHeavy'>{t.onboarding.actions.skipOnboarding}</Text>
      </Button>
      <TBotPrompt
        open={true}
        messages={messages}
        currentIndex={current}
        closeIcon={false}
        mode='onboarding'
        floating={false}
        onMessageRender={onMessageRender}
        onSkip={onSkip}
      />
    </StyledOnboardingContainer>
  )
}

export default OnboardingContainer
