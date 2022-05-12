import { config, useSpring } from 'react-spring'

import SvgClose from '../../styles/Icons/Close'
import TBot from '../TBot'

import { TBotPromptProps } from './types'

import {
  ContentRow,
  PromptContainer,
  StyledCloseIcon,
  TBotContainer,
  MessageContainer,
} from './styles'

/**
 * @name TBotPrompt
 *
 * @prop {boolean} open - controls rendering of prompt component
 * @prop {() => void} [onClose] - callback on close action of prompt
 * @prop {ReactNode} [children] - content rendered inside prompt component
 * @prop {string} [testid] - for testing
 */

const TBotPrompt = ({ open, onClose, children, testid }: TBotPromptProps) => {
  const promptAnim = useSpring({
    from: {
      opacity: 0,
    },
    opacity: 1,
    config: config.wobbly,
  })

  if (!open) {
    return null
  }

  return (
    <PromptContainer
      style={promptAnim}
      data-testid={testid || 'tbotprompt-cmp'}
    >
      <ContentRow>
        <MessageContainer>
          <StyledCloseIcon>
            <SvgClose fontSize={20} onClick={onClose} />
          </StyledCloseIcon>
          {children}
        </MessageContainer>
      </ContentRow>
      <TBotContainer>
        <TBot />
      </TBotContainer>
    </PromptContainer>
  )
}

export default TBotPrompt
