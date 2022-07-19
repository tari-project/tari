import HelpTip from '../../components/HelpTip'
import t from '../../locales'
import { useAppDispatch } from '../../store/hooks'
import { tbotactions } from '../../store/tbot'
import MessagesConfig from '../../config/helpMessagesConfig'

const BaseNodeHelp = () => {
  const dispatch = useAppDispatch()

  return (
    <HelpTip
      {...t.baseNode.helpMessages.howItWorks.tip}
      onHelp={() => dispatch(tbotactions.push(MessagesConfig.BaseNodeHelp))}
      header
    />
  )
}

export default BaseNodeHelp
