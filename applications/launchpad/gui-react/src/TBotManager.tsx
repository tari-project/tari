/* eslint-disable react/jsx-key */
import { useAppSelector } from './store/hooks'
import TBotPrompt from './components/TBot/TBotPrompt'
import { selectTBotQueue } from './store/tbot/selectors'
import { StyledMessage } from './components/TBot/TBotPrompt/styles'
import { Message1 } from './components/TBot/HelpComponents/CryptoMining'
import {
  Message1 as Merged1,
  Message2 as Merged2,
} from './components/TBot/HelpComponents/MergedMining'

const ComponentMap: { [key: string]: React.FC } = {
  cryptoHelpMessage1: Message1,
  mergedHelpMessage1: Merged1,
  mergedHelpMessage2: Merged2,
}

const TBotManager = () => {
  const tbotQueue = useAppSelector(selectTBotQueue)
  console.log('QUEUE: ', tbotQueue)
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
