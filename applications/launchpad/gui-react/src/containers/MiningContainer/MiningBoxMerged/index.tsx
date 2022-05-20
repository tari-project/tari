import { ReactNode, useEffect, useState } from 'react'
import { useTheme } from 'styled-components'
import { useAppSelector } from '../../../store/hooks'
import {
  selectMergedContainers,
  selectMergedMiningState,
  selectMergedSetupRequired,
} from '../../../store/mining/selectors'
import SvgMoneroSignet from '../../../styles/Icons/MoneroSignet'
import SvgTariSignet from '../../../styles/Icons/TariSignet'
import MiningBox from '../MiningBox'
import { MiningBoxStatus } from '../MiningBox/types'

import { Container } from '../../../store/containers/types'
// import SetupMerged from './SetupMerged'
import SetupMergedWithForm from './SetupMergedWithForm'
import MessagesConfig from '../../../config/helpMessagesConfig'

const MiningBoxMerged = () => {
  const theme = useTheme()

  let boxContent: ReactNode | undefined
  let currentStatus: MiningBoxStatus | undefined

  const nodeState = useAppSelector(selectMergedMiningState)
  console.log('NODE_STATE: ', nodeState)
  const containersState = useAppSelector(selectMergedContainers)
  const mergedSetupRequired = useAppSelector(selectMergedSetupRequired)

  // Stop only Merged related containers on pause/stop action
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
        c =>
          [Container.XMrig, Container.MMProxy, Container.Monerod].includes(
            c.type,
          ) && c.id,
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
    [MiningBoxStatus.Running]: {
      boxStyle: {
        background: theme.mergedGradient,
      },
      icon: {
        color: theme.accentMerged,
      },
    },
  }

  if (mergedSetupRequired) {
    currentStatus = MiningBoxStatus.SetupRequired
    /**
     * @TODO - switch between the following when onboarding is added
     */
    // boxContent = <SetupMerged mergedSetupRequired={mergedSetupRequired} />
    boxContent = (
      <SetupMergedWithForm mergedSetupRequired={mergedSetupRequired} />
    )
  }
  return (
    <MiningBox
      node='merged'
      icons={[
        <SvgMoneroSignet key='monero-icon' />,
        <SvgTariSignet key='tari-icon' />,
      ]}
      testId='merged-mining-box'
      statuses={statuses}
      currentStatus={currentStatus}
      nodeState={nodeState}
      containersState={containersState}
      containersToStopOnPause={containersToStopOnPause}
      helpMessages={MessagesConfig.mergedMiningHelp}
    >
      {boxContent}
    </MiningBox>
  )
}

export default MiningBoxMerged
