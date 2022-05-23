/* eslint-disable react/jsx-key */
import { useState } from 'react'
import TBotPrompt from '../../components/TBot/TBotPrompt'
import ChatDots from '../../components/TBot/DotsComponent'
import { HelpMessagesMap } from '../../config/helpMessagesConfig'
import { StyledMessage } from './styles'

/**
 * @name TBotManager
 *
 * Global component that handles all help prompt and notification messages
 */

const TBotManager = ({ messages }: { messages: string[] }) => {
  const [messageLoading, setMessageLoading] = useState<boolean>(true)

  const renderMessages = messages.map(msg => {
    if (HelpMessagesMap[msg] === undefined) {
      return <StyledMessage>{msg}</StyledMessage>
    }
    const Message = HelpMessagesMap[msg]
    return (
      <StyledMessage>
        <Message />
      </StyledMessage>
    )
  })

  return (
    <TBotPrompt open={messages.length > 0}>
      {renderMessages}
      {messageLoading && <ChatDots />}
    </TBotPrompt>
  )
}

export default TBotManager
