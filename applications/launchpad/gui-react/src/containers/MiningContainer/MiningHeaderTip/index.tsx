import t from '../../../locales'

import Button from '../../../components/Button'
import Text from '../../../components/Text'

import SvgStar from '../../../styles/Icons/Star'
import SvgInfo1 from '../../../styles/Icons/Info1'
import { StyledMiningHeaderTip } from './styles'

import { useAppDispatch } from '../../../store/hooks'
import { tbotactions } from '../../../store/tbot'

import {
  selectTariContainers,
  selectTariMiningState,
} from '../../../store/mining/selectors'

import MessagesConfig from '../../../config/helpMessagesConfig'
import { useAppSelector } from '../../../store/hooks'

/**
 * Renders instructions above mining node boxes
 */

const MiningHeaderTip = () => {
  const dispatch = useAppDispatch()

  const tariMiningState = useAppSelector(selectTariMiningState)
  const tariContainers = useAppSelector(selectTariContainers)

  let text = t.mining.headerTips.oneStepAway

  if (tariContainers.running) {
    text = t.mining.headerTips.runningOn
  } else if (tariMiningState.sessions && tariMiningState.sessions.length > 0) {
    text = t.mining.headerTips.continueMining
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
              dispatch(tbotactions.push(MessagesConfig.CryptoMiningHelp))
            }
          >
            {t.mining.headerTips.wantToKnowMore}
          </Button>
        </Text>
      </Text>
    </StyledMiningHeaderTip>
  )
}

export default MiningHeaderTip
