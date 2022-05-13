import { useAppSelector } from './store/hooks'
import TBotPrompt from './components/TBot/TBotPrompt'
import { selectTBotQueue } from './store/tbot/selectors'

const TBotManager = () => {
  const tbotQueue = useAppSelector(selectTBotQueue)
  console.log(tbotQueue)

  return <TBotPrompt open={tbotQueue.length > 0} />
}

export default TBotManager
