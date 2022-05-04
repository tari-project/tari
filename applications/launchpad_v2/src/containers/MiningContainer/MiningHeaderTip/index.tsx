import t from '../../../locales'

import Button from '../../../components/Button'
import Text from '../../../components/Text'

import SvgStar from '../../../styles/Icons/Star'
import SvgInfo1 from '../../../styles/Icons/Info1'
import { StyledMiningHeaderTip } from './styles'

/**
 * @TODO - draft - add other states
 */
const MiningHeaderTip = () => {
  return (
    <StyledMiningHeaderTip>
      <SvgStar height={18} width={18} style={{ marginRight: 8 }} />
      <Text>{t.mining.headerTips.oneStepAway}</Text>
      <Button
        type='link'
        variant='text'
        href='https://google.com'
        rightIcon={<SvgInfo1 />}
      >
        {t.mining.headerTips.wantToKnowMore}
      </Button>
    </StyledMiningHeaderTip>
  )
}

export default MiningHeaderTip
