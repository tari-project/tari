/* eslint-disable react/jsx-key */
import { useAppSelector } from './store/hooks'
import TBotPrompt from './components/TBot/TBotPrompt'
import { selectTBotQueue } from './store/tbot/selectors'
import { StyledMessage } from './components/TBot/TBotPrompt/styles'
import TestComponent from './config/TestComponent'
import TestComponent2 from './config/TestComponent2'

const ComponentMap: { [key: string]: React.FC } = {
  testComponent: TestComponent,
  testComponent2: TestComponent2,
}

const TBotManager = () => {
  const tbotQueue = useAppSelector(selectTBotQueue)

  const renderMessages = tbotQueue.map(msg => {
    if (ComponentMap[msg] === undefined) {
      return <StyledMessage>{msg}</StyledMessage>
    }
    const Message = ComponentMap[msg]
    return (
      <StyledMessage>
        <Message />
      </StyledMessage>
    )
  })

  return <TBotPrompt open={tbotQueue.length > 0}>{renderMessages}</TBotPrompt>
}

export default TBotManager
