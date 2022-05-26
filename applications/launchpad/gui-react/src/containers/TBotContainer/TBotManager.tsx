/* eslint-disable react/jsx-key */
import { useState, useEffect, useMemo, ReactNode } from 'react'
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
  const [messageLoading, setMessageLoading] = useState<boolean>(false)
  const [count, setCount] = useState(0)

  useEffect(() => {
    let counter = count
    let timeout: NodeJS.Timeout
    const interval = setInterval(() => {
      if (counter >= messages.length) {
        setMessageLoading(false)
        clearInterval(interval)
      } else if (messages.length > 0) {
        setMessageLoading(true)
        timeout = setTimeout(() => {
          setMessageLoading(false)
          setCount(count => count + 1)
          counter++
        }, 3000)
      }
    }, 6000)
    return () => {
      clearInterval(interval)
      clearTimeout(timeout)
      setCount(0)
      setMessageLoading(false)
    }
  }, [messages])

  const renderedMessages = useMemo(() => {
    return messages.slice(0, count).map(msg => {
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
  }, [messages, count]) as ReactNode

  return (
    <TBotPrompt open={messages.length > 0}>
      {renderedMessages}
      {messageLoading && <ChatDots />}
    </TBotPrompt>
  )
}

export default TBotManager
