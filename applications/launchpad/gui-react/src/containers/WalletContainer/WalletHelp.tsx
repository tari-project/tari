import HelpTip from '../../components/HelpTip'
import t from '../../locales'
import { useAppDispatch } from '../../store/hooks'
import { tbotactions } from '../../store/tbot'
import MessagesConfig from '../../config/helpMessagesConfig'

const WalletHelp = () => {
  const dispatch = useAppDispatch()

  return (
    <HelpTip
      {...t.wallet.helpMessages.howItWorks.tip}
      onHelp={() => dispatch(tbotactions.push(MessagesConfig.WalletHelp))}
    />
  )
}

export default WalletHelp
