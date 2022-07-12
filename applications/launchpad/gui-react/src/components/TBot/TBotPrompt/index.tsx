import { useState, useEffect, useRef, ReactNode, useMemo } from 'react'
import { config, useSpring } from 'react-spring'
import { appWindow } from '@tauri-apps/api/window'

import SvgClose from '../../../styles/Icons/Close'
import TBot from '..'

import { useAppDispatch, useAppSelector } from '../../../store/hooks'
import { tbotactions } from '../../../store/tbot'
import { TBotMessage, TBotPromptProps } from './types'

import {
  ContentRow,
  PromptContainer,
  StyledCloseIcon,
  TBotContainer,
  MessageContainer,
  StyledCloseContainer,
  ContentContainer,
  FadeOutSection,
  MessageWrapper,
  ScrollWrapper,
  HeightAnimationWrapper,
  TBotProgressContainer,
  PROMPT_HEIGHT_SPACING,
  TBotContainerSizes,
} from './styles'

import { ChatDotsLight, ChatDotsDark } from '../DotsComponent'
import MessageBox from './MessageBox'
import ProgressIndicator from '../../Onboarding/ProgressIndicator'
import { selectTheme } from '../../../store/app/selectors'

// The default time between rendering messages
const WAIT_TIME = 2800

/**
 * @name TBotPrompt
 *
 * @prop {boolean} open - controls rendering of prompt component
 * @prop {() => void} [onClose] - callback on close action of prompt
 * @prop {ReactNode} [children] - content rendered inside prompt component
 * @prop {string} [testid] - for testing
 * @prop {number} [currentIndex] -
 * @prop {boolean} [closeIcon] - controls rendering of close button
 * @prop {'help' | 'onboarding'} [mode] - usage mode for TBotPrompt
 * @prop {boolean} [onDarkBg=false] - is TBot rendered over the dark background?
 * @prop {'no' | 'yes'| 'dynamic'} [withFadeOutSection='dynamic'] - controls whether the top fading effect is rendered.
 *      - 'no' - do not render at all
 *      - 'yes' - render always
 *      - 'dynamic' - will render if the prompt height is large enough
 * @prop {(index: number) => void} [onMessageRender] - callback triggered after rendering of each message
 * @prop {() => void} [onSkip] - on skip chatting button click
 */

const TBotPrompt = ({
  open,
  floating,
  testid,
  messages,
  currentIndex = 1,
  closeIcon = true,
  mode = 'help',
  onDarkBg = false,
  withFadeOutSection = 'dynamic',
  onMessageRender,
  onSkip,
}: TBotPromptProps) => {
  const dispatch = useAppDispatch()
  const currentTheme = useAppSelector(selectTheme)

  const lastMsgRef = useRef<HTMLDivElement>(null)
  const scrollRef = useRef<HTMLDivElement>(null)
  const messageWrapperRef = useRef<HTMLDivElement>(null)
  const currentIndexRef = useRef<number>(currentIndex)

  const [messageLoading, setMessageLoading] = useState<boolean>(false)
  const [count, setCount] = useState(currentIndex || 0)
  const [height, setHeight] = useState(100)
  const [tickle, setTickle] = useState(true)
  const [showFadeOut, setShowFadeOut] = useState(withFadeOutSection === 'yes')
  const [progressFill, setProgressFill] = useState<number | undefined>(0)
  const [forceHeightCalculations, setForceHeightCalculations] = useState(false)

  const promptAnim = useSpring({
    from: {
      opacity: floating ? 1 : 0,
    },
    opacity: 1,
    config: config.wobbly,
  })

  const heightAnim = useSpring({
    maxHeight: height,
    duration: 50,
  })

  const fadeOutSectionAnim = useSpring({
    opacity:
      withFadeOutSection === 'yes' ||
      (withFadeOutSection === 'dynamic' && showFadeOut)
        ? 1
        : 0,
  })

  const scrollToBottom = () => {
    if (scrollRef.current !== null) {
      scrollRef.current.scrollTo({
        top: scrollRef.current.scrollHeight,
        behavior: 'smooth',
      })
    }
  }

  const close = () => {
    return dispatch(tbotactions.close())
  }

  const needToShowFadeOutSection = async () => {
    const size = await appWindow.innerSize()
    const fadeHeight =
      mode === 'help'
        ? TBotContainerSizes.sm.fadeOutHeight
        : TBotContainerSizes.md.fadeOutHeight
    setShowFadeOut(
      size.height * 0.9 - PROMPT_HEIGHT_SPACING - fadeHeight < height,
    )
  }

  const getChatDotsVersion = () => {
    if (currentTheme === 'light' && !onDarkBg) {
      return <ChatDotsLight />
    } else {
      return <ChatDotsDark />
    }
  }

  useEffect(() => {
    if (withFadeOutSection === 'dynamic') {
      needToShowFadeOutSection()
    }
  }, [height])

  useEffect(() => {
    // Update internal 'count' if parent changes the currentIndex
    if (currentIndex || currentIndex === 0) {
      setCount(currentIndex)
    }

    // If new currentIndex value is different, it means that we need to 'skip' next messages
    // and scroll to the bottom.
    if (currentIndexRef?.current && currentIndexRef?.current !== count) {
      setTimeout(() => scrollToBottom(), 800)
    }
  }, [currentIndex])

  // The following timer increases the 'count' - the messages indexer.
  // This way, tbot goes through the array of messages.
  useEffect(() => {
    let counter = count
    let timeout: NodeJS.Timeout

    if (messages && (messages.length === 1 || counter >= messages.length)) {
      setForceHeightCalculations(true)
    } else if (messages && counter >= messages.length) {
      setMessageLoading(false)
    } else if (messages && messages.length > 0) {
      setMessageLoading(true)
      // use custom waiting time, if previous message has 'wait' field.
      const lastMsg = messages[counter]
      let wait = WAIT_TIME
      if (
        lastMsg &&
        typeof lastMsg !== 'string' &&
        typeof lastMsg !== 'number' &&
        typeof lastMsg !== 'boolean' &&
        'wait' in lastMsg &&
        lastMsg.wait
      ) {
        wait = lastMsg.wait
      }

      // show loading dots, and then increase count which results in rendering next message.
      timeout = setTimeout(() => {
        setMessageLoading(false)
        counter++
        setCount(count => count + 1)
      }, wait)
    }

    return () => {
      clearTimeout(timeout)
      setMessageLoading(false)
    }
  }, [messages, count])
  // It will animate the list max-height. The timeout is needed, bc app has to render new content first,
  // so then we can learn what is the current list height, and animate the max-height of wrapping component.
  useEffect(() => {
    setTimeout(
      () => setHeight(messageWrapperRef?.current?.offsetHeight || 100),
      200,
    )
  }, [messageLoading, count])

  useEffect(() => {
    if (forceHeightCalculations) {
      setTimeout(
        () => setHeight(messageWrapperRef?.current?.offsetHeight || 100),
        200,
      )
      setForceHeightCalculations(false)
    }
  }, [forceHeightCalculations])

  // Tickle tbot whenever the app shows new message
  useEffect(() => {
    if (messageLoading) {
      setTimeout(() => {
        scrollToBottom()
      }, 400)
    } else {
      setTickle(true)
      setTimeout(() => {
        setTickle(false)
      }, 100)
    }
  }, [messageLoading])

  // Automatically scroll to the new message. Timeout is used to allow make some animations meanwhile.
  useEffect(() => {
    setTimeout(() => {
      if (lastMsgRef?.current) {
        lastMsgRef?.current.scrollIntoView({ block: 'start' })
        if (onMessageRender) {
          onMessageRender(count)
        }
      }
    }, 500)
  }, [lastMsgRef, lastMsgRef?.current])

  // Build messages list
  const renderedMessages = useMemo(() => {
    return messages?.slice(0, count).map((msg, idx) => {
      const counter = count < messages.length ? count - 1 : messages.length - 1
      const progressBarFill = (messages[counter] as TBotMessage).barFill
      setProgressFill(progressBarFill)
      let skipButtonCheck
      const msgTypeCheck =
        typeof msg !== 'string' &&
        typeof msg !== 'number' &&
        typeof msg !== 'boolean' &&
        msg
      if (msgTypeCheck && 'noSkip' in msg) {
        skipButtonCheck = false
      } else if (count === idx + 1) {
        skipButtonCheck = true
      }
      if (msgTypeCheck) {
        if ('content' in msg && typeof msg.content === 'function') {
          const FuncComponentMsg = msg.content
          return (
            <MessageBox
              key={idx}
              animate={count === idx + 1}
              ref={count === idx + 1 ? lastMsgRef : null}
              skipButton={mode === 'onboarding' && skipButtonCheck}
              onSkip={onSkip}
              floating={floating}
              $onDarkBg={onDarkBg}
            >
              <FuncComponentMsg />
            </MessageBox>
          )
        }
        return (
          <MessageBox
            key={idx}
            animate={count === idx + 1}
            ref={count === idx + 1 ? lastMsgRef : null}
            skipButton={mode === 'onboarding' && skipButtonCheck}
            onSkip={onSkip}
            floating={floating}
            $onDarkBg={onDarkBg}
          >
            {'content' in msg ? (msg.content as ReactNode | string) : msg}
          </MessageBox>
        )
      }

      return (
        <MessageBox
          key={idx}
          animate={count === idx + 1}
          ref={count === idx + 1 ? lastMsgRef : null}
          skipButton={mode === 'onboarding' && skipButtonCheck}
          onSkip={onSkip}
          floating={floating}
          $onDarkBg={onDarkBg}
        >
          {msg}
        </MessageBox>
      )
    })
  }, [messages, count]) as ReactNode

  if (!open) {
    return null
  }

  return (
    <PromptContainer
      style={promptAnim}
      $floating={floating}
      data-testid={testid || 'tbotprompt-cmp'}
    >
      <ContentRow>
        <ContentContainer $floating={floating}>
          <MessageContainer>
            <ScrollWrapper ref={scrollRef}>
              <HeightAnimationWrapper style={heightAnim}>
                <MessageWrapper ref={messageWrapperRef}>
                  {renderedMessages}
                </MessageWrapper>
              </HeightAnimationWrapper>
            </ScrollWrapper>
            {messageLoading && getChatDotsVersion()}
          </MessageContainer>
          <FadeOutSection
            $floating={floating}
            $onDarkBg={onDarkBg || currentTheme === 'dark'}
            style={fadeOutSectionAnim}
          />
        </ContentContainer>
        {closeIcon && (
          <StyledCloseContainer>
            <StyledCloseIcon>
              <SvgClose fontSize={20} onClick={close} />
            </StyledCloseIcon>
          </StyledCloseContainer>
        )}
      </ContentRow>
      <TBotProgressContainer mode={mode}>
        {mode === 'onboarding' && (
          <ProgressIndicator overallFill={progressFill} />
        )}
        <TBotContainer>
          <TBot animate={tickle} />
        </TBotContainer>
      </TBotProgressContainer>
    </PromptContainer>
  )
}

export default TBotPrompt
