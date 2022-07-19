import { ReactNode, useState } from 'react'
import { useTheme } from 'styled-components'
import { useAppSelector } from '../../../store/hooks'
import {
  selectMergedContainers,
  selectMergedMiningState,
  selectMergedSetupRequired,
  selectMergedUseAuth,
} from '../../../store/mining/selectors'
import SvgMoneroSignet from '../../../styles/Icons/MoneroSignet'
import SvgTariSignet from '../../../styles/Icons/TariSignet'
import MiningBox from '../MiningBox'
import { MiningBoxStatus } from '../MiningBox/types'

import t from '../../../locales'

// import SetupMerged from './SetupMerged'
import SetupMergedWithForm from './SetupMergedWithForm'
import MessagesConfig from '../../../config/helpMessagesConfig'
import {
  BestChoiceTagIcon,
  BestChoiceTagText,
  StyledBestChoiceTag,
} from './styles'
import { selectAreMoneroCredentialsPresent } from '../../../store/credentials/selectors'

const BestChoiceTag = () => {
  return (
    <StyledBestChoiceTag>
      <BestChoiceTagText>{t.common.phrases.bestChoice} </BestChoiceTagText>
      <BestChoiceTagIcon>💪</BestChoiceTagIcon>
    </StyledBestChoiceTag>
  )
}

const MiningBoxMerged = () => {
  const theme = useTheme()

  const [bestChoiceTag, setBestChoiceTag] = useState(false)

  let boxContent: ReactNode | undefined
  let currentStatus: MiningBoxStatus | undefined

  const nodeState = useAppSelector(selectMergedMiningState)
  const containersState = useAppSelector(selectMergedContainers)
  const mergedSetupRequired = useAppSelector(selectMergedSetupRequired)
  const mergedAuthentication = useAppSelector(selectMergedUseAuth)

  const credentials = useAppSelector(selectAreMoneroCredentialsPresent)

  const statuses = {
    [MiningBoxStatus.SetupRequired]: {
      tag: {
        content: bestChoiceTag ? (
          <BestChoiceTag />
        ) : (
          t.common.phrases.readyToSet
        ),
      },
    },
    [MiningBoxStatus.PausedNoSession]: {
      tag: {
        content: t.common.phrases.readyToGo,
      },
    },
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
    boxContent = (
      <SetupMergedWithForm
        mergedSetupRequired={mergedSetupRequired}
        changeTag={() => setBestChoiceTag(true)}
      />
    )
  }
  return (
    <MiningBox
      node='merged'
      icons={[
        { coin: 'xmr', component: <SvgMoneroSignet key='monero-icon' /> },
        { coin: 'xtr', component: <SvgTariSignet key='tari-icon' /> },
      ]}
      testId='merged-mining-box'
      statuses={statuses}
      currentStatus={currentStatus}
      nodeState={nodeState}
      containersState={containersState}
      helpMessages={MessagesConfig.MergedMiningHelp}
      requiredAuthentication={{
        wallet: true,
        monero: mergedAuthentication && !credentials,
      }}
    >
      {boxContent}
    </MiningBox>
  )
}

export default MiningBoxMerged
