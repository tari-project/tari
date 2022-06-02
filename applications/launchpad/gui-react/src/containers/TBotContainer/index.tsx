import TBotPrompt from '../../components/TBot/TBotPrompt'
import { useAppSelector } from '../../store/hooks'
import { selectTBotQueue } from '../../store/tbot/selectors'

const TBotContainer = () => {
  const tbotQueue = useAppSelector(selectTBotQueue)

  return (
    <TBotPrompt open={tbotQueue.length > 0} messages={tbotQueue} floating />
  )
}

export default TBotContainer
