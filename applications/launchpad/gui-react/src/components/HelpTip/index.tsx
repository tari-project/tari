import Button from '../../components/Button'
import Text from '../../components/Text'

import SvgStar from '../../styles/Icons/Star'
import SvgInfo1 from '../../styles/Icons/Info1'
import { StyledHelpTipWrapper } from './styles'

/**
 * @name HelpTip
 * @description renders help tip with call to action button to open help
 *
 * @prop {string} text - text introducing help
 * @prop {string} cta - call to action text inside button
 * @prop {() => void} onHelp - callback called when user interacts with cta
 */
const HelpTip = ({
  text,
  cta,
  onHelp,
}: {
  text: string
  cta: string
  onHelp: () => void
}) => (
  <StyledHelpTipWrapper data-testid='mining-header-tip-cmp'>
    <SvgStar height={24} width={24} style={{ marginRight: 8 }} />
    <Text type='defaultHeavy'>
      {text}{' '}
      <Text as='span' type='defaultMedium'>
        <Button
          variant='button-in-text'
          rightIcon={<SvgInfo1 width='20px' height='20px' />}
          autosizeIcons={false}
          onClick={onHelp}
        >
          {cta}
        </Button>
      </Text>
    </Text>
  </StyledHelpTipWrapper>
)

export default HelpTip
