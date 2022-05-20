/* eslint-disable react/jsx-key */
import TBotPrompt from '../../components/TBot/TBotPrompt'
import { StyledMessage } from './styles'
import { HelpMessagesMap } from '../../config/helpMessagesConfig'

/**
 * @name TBotManager
 *
 * Global component that handles all help prompt and notification messages
 */

const TBotManager = ({ messages }: { messages: string[] }) => {
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

  return <TBotPrompt open={messages.length > 0}>{renderMessages}</TBotPrompt>
}

export default TBotManager
