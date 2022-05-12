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
import { config, useSpring } from 'react-spring'

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
