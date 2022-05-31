import TBotPrompt from '../../components/TBot/TBotPrompt'
import { useAppSelector } from '../../store/hooks'
import { selectTBotOpen, selectTBotQueue } from '../../store/tbot/selectors'

const TBotContainer = () => {
  const tbotQueue = useAppSelector(selectTBotQueue)
  const tbotOpen = useAppSelector(selectTBotOpen)

  return <TBotPrompt open={tbotOpen} messages={tbotQueue} floating />
}

export default TBotContainer
