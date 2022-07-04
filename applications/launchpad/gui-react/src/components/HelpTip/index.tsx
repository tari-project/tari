import Button from '../../components/Button'
import Text from '../../components/Text'
import SvgStar from '../../styles/Icons/Star'
import SvgInfo1 from '../../styles/Icons/Info1'

import { StyledHelpTipWrapper } from './styles'
import { HelpTipProps } from './types'

/**
 * @name HelpTip
 * @description renders help tip with call to action button to open help
 *
 * @prop {string} text - text introducing help
 * @prop {string} cta - call to action text inside button
 * @prop {() => void} onHelp - callback called when user interacts with cta
 * @prop {CSSProperties} [style] - styles to apply to main wrapper element
 * @prop {boolean} [header] - whether the help tip should be rendered with additional top/bottom margin suitable for headers
 */
const HelpTip = ({ text, cta, onHelp, style, header }: HelpTipProps) => {
  return (
    <StyledHelpTipWrapper
      data-testid='mining-header-tip-cmp'
      style={style}
      header={header}
    >
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
}

export default HelpTip
