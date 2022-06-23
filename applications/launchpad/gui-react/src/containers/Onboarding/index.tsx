import { useState, useEffect, useRef } from 'react'
import Button from '../../components/Button'
import TBotPrompt from '../../components/TBot/TBotPrompt'
import { TBotMessage } from '../../components/TBot/TBotPrompt/types'
import Text from '../../components/Text'
import {
  OnboardingMessagesIntro,
  OnboardingMessagesDockerInstall,
  OnboardingMessagesDockerInstallAfter,
  DownloadImagesMessage,
  DownloadImagesErrorMessage,
  OnboardingMessagesLastSteps,
} from '../../config/onboardingMessagesConfig'
import { setOnboardingComplete } from '../../store/app'
import { useAppDispatch } from '../../store/hooks'
import { StyledOnboardingContainer } from './styles'

const OnboardingContainer = () => {
  const dispatch = useAppDispatch()

  const messagesRef = useRef<TBotMessage[]>()

  const [messages, setMessages] = useState(OnboardingMessagesIntro)
  const [dockerInstalled, setDockerInstalled] = useState<boolean | undefined>(
    undefined,
  )
  const [current, setCurrent] = useState(1)

  messagesRef.current = messages

  const pushMessages = (msgs: TBotMessage[]) => {
    if (!messagesRef.current) {
      return
    }
    const newMsgs = messagesRef.current.concat(msgs)
    setMessages(newMsgs)
  }

  /** MOCK FOR DOCKER IMAGE DOWNLOAD */
  const onImagesDowloadSuccess = () => {
    pushMessages(OnboardingMessagesLastSteps)
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
      },
    ])
  }
  /** END OF MOCK FOR DOCKER IMAGE DOWNLOAD */

  /** MOCK FOR DOCKER INSTALLATION */
  useEffect(() => {
    if (dockerInstalled === false) {
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
  }, [dockerInstalled])

  const onMessageRender = (index: number) => {
    if (index === 4) {
      setTimeout(() => {
        pushMessages([
          {
            content: (
              <>
                <Text>Is docker installed?</Text>
                <Button onClick={() => setDockerInstalled(true)}>Yes</Button>
                <Button onClick={() => setDockerInstalled(false)}>No</Button>
              </>
            ),
            barFill: 0.25,
            noSkip: true,
          },
        ])
      }, 500)
    }
  }
  /** END OF MOCK FOR DOCKER INSTALLATION */

  const onSkip = () => {
    setCurrent(messages.length)
  }

  return (
    <StyledOnboardingContainer>
      {/**
       * @TODO remove this temporary button after onboarding development is done.
       */}
      <Button
        onClick={() => dispatch(setOnboardingComplete(true))}
        style={{
          position: 'absolute',
          bottom: 40,
          left: 40,
        }}
      >
        EXIT TO HOME
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
