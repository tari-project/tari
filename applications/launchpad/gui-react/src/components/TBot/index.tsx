import SvgTBotBase from '../../styles/Icons/TBotBase'
import SvgTBotHearts from '../../styles/Icons/TBotHearts'
import SvgTBotHeartsMonero from '../../styles/Icons/TBotHeartsMonero'
import SvgTBotLoading from '../../styles/Icons/TBotLoading'
import SvgTBotRadar from '../../styles/Icons/TBotRadar'

import { TBotProps } from './types'

/**
 * TBot component
 *
 * @prop {TBotType} [type] - TBot variant to render
 * @prop {number} [size] - fontSize of TBot
 * @prop {CSSProperties} [style] - optional TBot additional styling
 *
 * @example
 * <TBot type='hearts' size={64} />
 */

const TBot = ({ type = 'base', size, style }: TBotProps) => {
  const botVariants = {
    base: SvgTBotBase,
    hearts: SvgTBotHearts,
    heartsMonero: SvgTBotHeartsMonero,
    loading: SvgTBotLoading,
    radar: SvgTBotRadar,
  }

  const TBotComponent = botVariants[type]

  return <TBotComponent fontSize={size} style={style} data-testid='tbot-cmp' />
}

export default TBot
