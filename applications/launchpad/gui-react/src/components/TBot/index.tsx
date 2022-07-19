import { useSpring } from 'react-spring'
import { useTheme } from 'styled-components'

import SvgTBotBase from '../../styles/Icons/TBotBase'
import SvgTBotHeartsMonero from '../../styles/Icons/TBotHeartsMonero'
import SvgTBotLoading from '../../styles/Icons/TBotLoading'
import SvgTBotRadar from '../../styles/Icons/TBotSearch'
import SvgTBotHearts from '../../styles/Icons/TBotHearts'

import { TBotContainer, TBotScaleContainer, TBotShadow } from './styles'
import { TBotProps, CSSShadowDefinition } from './types'

/**
 * TBot component
 *
 * @prop {TBotType} [type] - TBot variant to render
 * @prop {CSSProperties} [style] - optional TBot additional styling
 * @prop {boolean} [animate] - optional prop to trigger the new message T-Bot animation, set to true to trigger animation
 * @prop {boolean | ShadowDefinition} [shadow] - optional prop to define shadow dropped around TBot, use true for defaults (color: theme.accent, spread: 10, blur: 100)
 * @prop {boolean} [disableEnterAnimation] - optional prop to disable enter animation
 *
 * @example
 * <TBot type='hearts' style={{ fontSize: '24px' }} animate={triggerAnimation} />
 */
const TBot = ({
  type = 'base',
  style,
  animate,
  shadow,
  disableEnterAnimation,
}: TBotProps) => {
  const theme = useTheme()
  const { fontSize } = { fontSize: 74, ...style }
  const defaultShadow: CSSShadowDefinition = {
    color: theme.accent,
    spread: 10,
    blur: 100,
    size: parseInt(fontSize.toString()),
  }

  const botVariants = {
    base: SvgTBotBase,
    hearts: SvgTBotHearts,
    heartsMonero: SvgTBotHeartsMonero,
    loading: SvgTBotLoading,
    search: SvgTBotRadar,
  }

  const enterAnim = disableEnterAnimation
    ? undefined
    : useSpring({
        from: { width: '0px', height: '0px' },
        to: {
          width: '100%',
          height: '100%',
        },
        config: {
          duration: 100,
        },
      })

  const newTBotMessageAnimation = useSpring({
    from: { transform: 'scale(1)' },
    to: {
      transform: animate ? 'scale(1.5)' : 'scale(1)',
      transition: 'all ease-in-out',
    },
    config: {
      duration: 300,
    },
    loop: {
      transform: 'scale(1)',
    },
  })

  const shadowDefinition: CSSShadowDefinition =
    !shadow || shadow === true ? defaultShadow : { ...defaultShadow, ...shadow }

  const TBotComponent = botVariants[type]

  return (
    <TBotContainer
      style={newTBotMessageAnimation}
      shadow={shadow ? shadowDefinition : undefined}
    >
      <TBotScaleContainer style={enterAnim}>
        {shadow && <TBotShadow shadow={shadowDefinition} />}
        <TBotComponent
          fontSize={fontSize}
          style={{ ...style, zIndex: 1 }}
          data-testid='tbot-cmp'
        />
      </TBotScaleContainer>
    </TBotContainer>
  )
}

export default TBot
