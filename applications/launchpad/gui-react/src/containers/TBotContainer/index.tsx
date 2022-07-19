import { useMemo, ReactNode } from 'react'
import TBotPrompt from '../../components/TBot/TBotPrompt'
import { useAppSelector } from '../../store/hooks'
import { selectTBotQueue } from '../../store/tbot/selectors'
import { TBotMessage } from '../../components/TBot/TBotPrompt/types'
import { HelpMessagesMap } from '../../config/helpMessagesConfig'
import { selectExpertView } from '../../store/app/selectors'

const TBotContainer = () => {
  const tbotQueue = useAppSelector(selectTBotQueue)
  const expertView = useAppSelector(selectExpertView)

  const messages: (string | ReactNode | TBotMessage)[] = useMemo(() => {
    return tbotQueue.map(msg => HelpMessagesMap[msg])
  }, [tbotQueue])
  return (
    <TBotPrompt
      open={tbotQueue.length > 0}
      messages={messages}
      floating
      onDarkBg={expertView !== 'hidden'}
    />
  )
}

export default TBotContainer
