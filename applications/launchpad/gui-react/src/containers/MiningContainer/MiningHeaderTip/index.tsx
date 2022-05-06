import t from '../../../locales'

import Button from '../../../components/Button'
import Text from '../../../components/Text'

import SvgStar from '../../../styles/Icons/Star'
import SvgInfo1 from '../../../styles/Icons/Info1'
import { StyledMiningHeaderTip } from './styles'
import { useSelector } from 'react-redux'
import {
  selectLastSession,
  selectTariMiningStatus,
} from '../../../store/mining/selectors'
import { MiningNodesStatus } from '../../../store/mining/types'
import { RootState } from '../../../store'

/**
 * Renders instructions above mining node boxes
 */
const MiningHeaderTip = () => {
  const tariMiningStatus = useSelector(selectTariMiningStatus)
  const lastSession = useSelector((state: RootState) =>
    selectLastSession(state, 'tari'),
  )

  let text = t.mining.headerTips.oneStepAway

  switch (tariMiningStatus) {
    case MiningNodesStatus.RUNNING:
      text = t.mining.headerTips.runningOn
      break
    case MiningNodesStatus.PAUSED:
      if (lastSession) {
        text = t.mining.headerTips.continueMining
      }
      break
    default:
      break
  }

  return (
    <StyledMiningHeaderTip data-testid='mining-header-tip-cmp'>
      <SvgStar height={24} width={24} style={{ marginRight: 8 }} />
      <Text type='defaultHeavy'>
        {text}{' '}
        <Text as='span' type='defaultMedium'>
          <Button type='button-in-text' variant='text' rightIcon={<SvgInfo1 />}>
            {t.mining.headerTips.wantToKnowMore}
          </Button>
        </Text>
      </Text>
    </StyledMiningHeaderTip>
  )
}

export default MiningHeaderTip
