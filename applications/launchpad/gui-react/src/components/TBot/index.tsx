import { useSpring, config } from 'react-spring'
import SvgTBotBase from '../../styles/Icons/TBotBase'
import SvgTBotHearts from '../../styles/Icons/TBotHearts'
import SvgTBotHeartsMonero from '../../styles/Icons/TBotHeartsMonero'
import SvgTBotLoading from '../../styles/Icons/TBotLoading'
import SvgTBotRadar from '../../styles/Icons/TBotSearch'
import { TBotContainer, TBotScaleContainer } from './styles'

import { TBotProps } from './types'

/**
 * TBot component
 *
 * @prop {TBotType} [type] - TBot variant to render
 * @prop {CSSProperties} [style] - optional TBot additional styling
 * @prop {boolean} [animate] - optional prop to trigger the new message T-Bot animation
 *
 * @example
 * <TBot type='hearts' style={{ fontSize: '24px' }} />
 */

const TBot = ({ type = 'base', style, animate }: TBotProps) => {
  const botVariants = {
    base: SvgTBotBase,
    hearts: SvgTBotHearts,
    heartsMonero: SvgTBotHeartsMonero,
    loading: SvgTBotLoading,
    search: SvgTBotRadar,
  }

  // Animation for T-Bot first render
  const enterAnim = useSpring({
    from: { width: '0px', height: '0px' },
    to: {
      width: '100%',
      height: '100%',
    },
    config: config.wobbly,
  })

  // Animation for new T-Bot messages
  const scaleAnim = useSpring({
    from: { transform: 'scale(1)' },
    to: {
      transform: animate ? 'scale(1.2)' : 'scale(1)',
      transition: 'all ease-in-out',
    },
    config: {
      duration: 200,
    },
    loop: {
      transform: 'scale(1)',
    },
  })

  const TBotComponent = botVariants[type]

  return (
    <TBotContainer style={scaleAnim}>
      <TBotScaleContainer style={enterAnim}>
        <TBotComponent fontSize={74} style={style} data-testid='tbot-cmp' />
      </TBotScaleContainer>
    </TBotContainer>
  )
}

export default TBot
