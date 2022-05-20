import { useState, useEffect } from 'react'
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

const TBotPrompt = ({ open, children, testid }: TBotPromptProps) => {
  const [multipleMessages, setMultipleMessages] = useState(false)
  const dispatch = useAppDispatch()
  const promptAnim = useSpring({
    from: {
      opacity: 0,
    },
    opacity: 1,
    config: config.wobbly,
  })

  // @TODO: need to assess if this needed, probably isn't
  // useEffect(() => {
  //   if (children && children.length > 1) {
  //     setMultipleMessages(true)
  //   } else {
  //     setMultipleMessages(false)
  //   }
  // }, [children])

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
          <MessageContainer multi={multipleMessages}>
            {children}
          </MessageContainer>
        </ContentContainer>
      </ContentRow>
      <TBotContainer>
        <TBot />
      </TBotContainer>
    </PromptContainer>
  )
}

export default TBotPrompt
