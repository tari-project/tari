import { useAppSelector } from '../../store/hooks'
import { selectTBotQueue } from '../../store/tbot/selectors'
import TBotManager from './TBotManager'

const TBotContainer = () => {
  const tbotQueue = useAppSelector(selectTBotQueue)

  return <TBotManager messages={tbotQueue} />
}

export default TBotContainer
