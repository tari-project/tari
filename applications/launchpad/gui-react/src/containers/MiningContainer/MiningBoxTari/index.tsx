import { ReactNode } from 'react'
import { useSelector } from 'react-redux'
import { selectTariMiningStatus } from '../../../store/mining/selectors'
import { MiningNodesStatus } from '../../../store/mining/types'
import SvgTariSignet from '../../../styles/Icons/TariSignet'
import WalletPasswordWizard from '../../WalletPasswordWizard'
import MiningBox from '../MiningBox'

const MiningBoxTari = () => {
  const tariNodeStatus = useSelector(selectTariMiningStatus)

  let boxContent: ReactNode | undefined

  if (tariNodeStatus === MiningNodesStatus.SETUP_REQUIRED) {
    boxContent = <WalletPasswordWizard />
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
