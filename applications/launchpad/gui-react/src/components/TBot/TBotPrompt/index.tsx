import { useState, useEffect, useRef } from 'react'

import { config, useSpring } from 'react-spring'

import SvgClose from '../../../styles/Icons/Close'
import TBot from '..'

import { useAppDispatch } from '../../../store/hooks'
import { tbotactions } from '../../../store/tbot'
import { TBotPromptProps } from './types'

import {
  ContentRow,
  PromptContainer,
  StyledCloseIcon,
  TBotContainer,
  MessageContainer,
  StyledCloseContainer,
  ContentContainer,
  FadeOutSection,
} from './styles'

/**
 * @name TBotPrompt
 *
 * @prop {boolean} open - controls rendering of prompt component
 * @prop {() => void} [onClose] - callback on close action of prompt
 * @prop {ReactNode} [children] - content rendered inside prompt component
 * @prop {string} [testid] - for testing
 */

const TBotPrompt = ({ open, children, animate, testid }: TBotPromptProps) => {
  const [multipleMessages, setMultipleMessages] = useState(false)

  const dispatch = useAppDispatch()
  const promptAnim = useSpring({
    from: {
      opacity: 0,
    },
    opacity: 1,
    config: config.wobbly,
  })

  const scrollRef = useRef<HTMLDivElement>(null)

  const scrollToBottom = () => {
    if (scrollRef.current !== null) {
      scrollRef.current.scrollTo({
        top: scrollRef.current.scrollHeight,
        behavior: 'smooth',
      })
    }
  }
  useEffect(() => {
    scrollToBottom()
  }, [children])

  const close = () => {
    return dispatch(tbotactions.close())
  }

  if (!open) {
    return null
  }

  return (
    <PromptContainer
      style={promptAnim}
      data-testid={testid || 'tbotprompt-cmp'}
    >
      <ContentRow>
        <ContentContainer>
          <FadeOutSection />
          <StyledCloseContainer>
            <StyledCloseIcon>
              <SvgClose fontSize={20} onClick={close} />
            </StyledCloseIcon>
          </StyledCloseContainer>
          <MessageContainer multi={multipleMessages} ref={scrollRef}>
            {children}
          </MessageContainer>
        </ContentContainer>
      </ContentRow>
      <TBotContainer>
        <TBot animate={animate} />
      </TBotContainer>
    </PromptContainer>
  )
}

export default TBotPrompt
