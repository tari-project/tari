import { ReactNode } from 'react'
import { useSelector } from 'react-redux'

import MiningBox from '../MiningBox'
import WalletPasswordWizard from '../../WalletPasswordWizard'

import SvgTariSignet from '../../../styles/Icons/TariSignet'

import t from '../../../locales'

import { useAppDispatch } from '../../../store/hooks'
import { actions as miningActions } from '../../../store/mining'
import { selectTariMiningStatus } from '../../../store/mining/selectors'
import { MiningNodesStatus } from '../../../store/mining/types'

const MiningBoxTari = () => {
  const dispatch = useAppDispatch()
  const tariNodeStatus = useSelector(selectTariMiningStatus)

  let boxContent: ReactNode | undefined

  if (tariNodeStatus === MiningNodesStatus.SETUP_REQUIRED) {
    boxContent = (
      <WalletPasswordWizard
        submitBtnText={t.mining.setUpTariWalletSubmitBtn}
        onSuccess={() =>
          dispatch(
            miningActions.setNodeStatus({
              node: 'tari',
              status: MiningNodesStatus.PAUSED,
            }),
          )
        }
      />
    )
  }

  return (
    <MiningBox
      node='tari'
      icons={[<SvgTariSignet key='tari-icon' />]}
      testId='tari-mining-box'
    >
      {boxContent}
    </MiningBox>
  )
}

export default MiningBoxTari
