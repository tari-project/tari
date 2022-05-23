import { ReactNode, useEffect, useState } from 'react'

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
import { Container } from '../../../store/containers/types'
import { useTheme } from 'styled-components'

const MiningBoxTari = () => {
  const theme = useTheme()

  const nodeState = useAppSelector(selectTariMiningState)
  const containersState = useAppSelector(selectTariContainers)
  const tariSetupRequired = useAppSelector(selectTariSetupRequired)

  // Stop only SHA3 miner on pause/stop action
  const [containersToStopOnPause, setContainersToStopOnPause] = useState<
    { id: string; type: Container }[]
  >([])

  useEffect(() => {
    if (
      (!containersState ||
        !containersState.dependsOn ||
        containersState.dependsOn.length === 0) &&
      containersToStopOnPause.length > 0
    ) {
      setContainersToStopOnPause([])
    } else if (containersState && containersState.dependsOn?.length > 0) {
      const cs = containersState.dependsOn.filter(
        c => [Container.SHA3Miner].includes(c.type) && c.id,
      )

      setContainersToStopOnPause(
        cs.map(c => ({
          id: c.id,
          type: c.type,
        })),
      )
    }
  }, [containersState])

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
      containersToStopOnPause={containersToStopOnPause}
    >
      {boxContent}
    </MiningBox>
  )
}

export default MiningBoxTari
