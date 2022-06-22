import HelpTip from '../../components/HelpTip'
import { HelpTipProps } from '../../components/HelpTip/types'
import t from '../../locales'
import { useAppDispatch } from '../../store/hooks'
import { tbotactions } from '../../store/tbot'
import MessagesConfig from '../../config/helpMessagesConfig'

const WalletHelp = ({ header }: Pick<HelpTipProps, 'header'>) => {
  const dispatch = useAppDispatch()

  return (
    <HelpTip
      {...t.wallet.helpMessages.howItWorks.tip}
      onHelp={() => dispatch(tbotactions.push(MessagesConfig.WalletHelp))}
      header={header}
    />
  )
}

export default WalletHelp
