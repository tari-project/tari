import t from '../../../locales'
import HelpTip from '../../../components/HelpTip'
import { useAppDispatch } from '../../../store/hooks'
import { tbotactions } from '../../../store/tbot'
import {
  selectTariContainers,
  selectTariMiningState,
  selectTariSetupRequired,
} from '../../../store/mining/selectors'
import MessagesConfig from '../../../config/helpMessagesConfig'
import { useAppSelector } from '../../../store/hooks'

/**
 * Renders instructions above mining node boxes
 */
const MiningHeaderTip = () => {
  const dispatch = useAppDispatch()

  const tariSetupRequired = useAppSelector(selectTariSetupRequired)
  const tariMiningState = useAppSelector(selectTariMiningState)
  const tariContainers = useAppSelector(selectTariContainers)

  let text = t.mining.headerTips.oneStepAway

  if (tariContainers.running) {
    text = t.mining.headerTips.runningOn
  } else if (tariSetupRequired) {
    text = t.mining.headerTips.oneStepAway
  } else if (!tariMiningState.session) {
    text = t.mining.headerTips.oneClickAway
  } else if (tariMiningState.session && tariMiningState.session.startedAt) {
    text = t.mining.headerTips.continueMining
  }

  return (
    <HelpTip
      text={text}
      cta={t.mining.headerTips.wantToKnowMore}
      onHelp={() => dispatch(tbotactions.push(MessagesConfig.CryptoMiningHelp))}
      header
    />
  )
}

export default MiningHeaderTip
