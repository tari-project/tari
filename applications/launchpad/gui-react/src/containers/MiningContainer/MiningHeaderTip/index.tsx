import t from '../../../locales'

import Button from '../../../components/Button'
import Text from '../../../components/Text'

import SvgStar from '../../../styles/Icons/Star'
import SvgInfo1 from '../../../styles/Icons/Info1'
import { StyledMiningHeaderTip } from './styles'
import { useSelector } from 'react-redux'
import { useAppDispatch } from '../../../store/hooks'
import { tbotactions } from '../../../store/tbot'
import {
  selectLastSession,
  selectTariMiningStatus,
} from '../../../store/mining/selectors'
import { MiningNodesStatus } from '../../../store/mining/types'
import { RootState } from '../../../store'
import MessagesConfig from '../../../config/helpMessagesConfig'

/**
 * Renders instructions above mining node boxes
 */

const MiningHeaderTip = () => {
  const tariMiningStatus = useSelector(selectTariMiningStatus)
  const lastSession = useSelector((state: RootState) =>
    selectLastSession(state, 'tari'),
  )
  const dispatch = useAppDispatch()

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
          <Button
            variant='button-in-text'
            rightIcon={<SvgInfo1 width='20px' height='20px' />}
            autosizeIcons={false}
            onClick={() =>
              dispatch(tbotactions.push(MessagesConfig.cryptoMiningHelp))
            }
          >
            {t.mining.headerTips.wantToKnowMore}
          </Button>
          {/* FOR DEV PURPOSES */}
          <Button
            variant='button-in-text'
            rightIcon={<SvgInfo1 width='20px' height='20px' />}
            autosizeIcons={false}
            onClick={() =>
              dispatch(tbotactions.push(MessagesConfig.mergedMiningHelp))
            }
          >
            TEST BUTTON
          </Button>
        </Text>
      </Text>
    </StyledMiningHeaderTip>
  )
}

export default MiningHeaderTip
