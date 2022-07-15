import { ReactNode } from 'react'

import MiningBox from '../MiningBox'
import { MiningBoxStatus } from '../MiningBox/types'

import WalletPasswordWizard from '../../WalletPasswordWizard'

import SvgTariSignet from '../../../styles/Icons/TariSignet'

import t from '../../../locales'

import { useAppSelector } from '../../../store/hooks'
import {
  selectTariContainers,
  selectTariMiningState,
  selectTariSetupRequired,
} from '../../../store/mining/selectors'
import { TariMiningSetupRequired } from '../../../store/mining/types'
import { useTheme } from 'styled-components'

const MiningBoxTari = () => {
  const theme = useTheme()

  const nodeState = useAppSelector(selectTariMiningState)
  const containersState = useAppSelector(selectTariContainers)
  const tariSetupRequired = useAppSelector(selectTariSetupRequired)

  const statuses = {
    [MiningBoxStatus.SetupRequired]: {
      boxStyle: {
        boxShadow: theme.shadow40,
        borderColor: 'transparent',
      },
    },
  }

  let boxContent: ReactNode | undefined
  let currentStatus: MiningBoxStatus | undefined

  if (tariSetupRequired === TariMiningSetupRequired.MissingWalletAddress) {
    currentStatus = MiningBoxStatus.SetupRequired
    boxContent = (
      <WalletPasswordWizard submitBtnText={t.mining.setUpTariWalletSubmitBtn} />
    )
  }

  return (
    <MiningBox
      node='tari'
      icons={[{ coin: 'xtr', component: <SvgTariSignet key='tari-icon' /> }]}
      testId='tari-mining-box'
      statuses={statuses}
      currentStatus={currentStatus}
      nodeState={nodeState}
      containersState={containersState}
      requiredAuthentication={{
        wallet: true,
      }}
    >
      {boxContent}
    </MiningBox>
  )
}

export default MiningBoxTari
