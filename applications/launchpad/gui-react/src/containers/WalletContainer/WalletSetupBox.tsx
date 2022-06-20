import { useTheme } from 'styled-components'

import { useAppSelector } from '../../store/hooks'
import SvgTariSignet from '../../styles/Icons/TariSignet'
import WalletPasswordWizard from '../../containers/WalletPasswordWizard'
import MiningBox from '../../containers/MiningContainer/MiningBox'
import { MiningBoxStatus } from '../../containers/MiningContainer/MiningBox/types'
import t from '../../locales'
import {
  selectTariContainers,
  selectTariMiningState,
} from '../../store/mining/selectors'

const WalletSetupBox = () => {
  const theme = useTheme()

  const nodeState = useAppSelector(selectTariMiningState)
  const containersState = useAppSelector(selectTariContainers)

  return (
    <MiningBox
      node='tari'
      icons={[{ coin: 'xtr', component: <SvgTariSignet key='tari-icon' /> }]}
      testId='tari-mining-box'
      statuses={{
        [MiningBoxStatus.SetupRequired]: {
          title: t.wallet.setUpTariWalletTitle,
          boxStyle: {
            boxShadow: theme.shadow40,
            borderColor: 'transparent',
          },
        },
      }}
      currentStatus={MiningBoxStatus.SetupRequired}
      nodeState={nodeState}
      containersState={containersState}
    >
      <WalletPasswordWizard submitBtnText={t.wallet.setUpTariWalletSubmitBtn} />
    </MiningBox>
  )
}

export default WalletSetupBox
