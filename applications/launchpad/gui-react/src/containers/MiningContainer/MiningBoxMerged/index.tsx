import { useTheme } from 'styled-components'
import SvgMoneroSignet from '../../../styles/Icons/MoneroSignet'
import SvgTariSignet from '../../../styles/Icons/TariSignet'
import MiningBox from '../MiningBox'

const MiningBoxMerged = () => {
  const theme = useTheme()

  const statuses = {
    RUNNING: {
      boxStyle: {
        background: theme.mergedGradient,
      },
      icon: {
        color: theme.accentMerged,
      },
    },
  }

  return (
    <MiningBox
      node='merged'
      statuses={statuses}
      icons={[
        <SvgMoneroSignet key='monero-icon' />,
        <SvgTariSignet key='tari-icon' />,
      ]}
      testId='merged-mining-box'
    />
  )
}

export default MiningBoxMerged
