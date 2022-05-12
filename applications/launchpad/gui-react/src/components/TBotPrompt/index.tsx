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

const TBotPrompt = ({ open, onClose, children }: TBotPromptProps) => {
  if (!open) {
    return null
  }

  return (
    <PromptContainer>
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
