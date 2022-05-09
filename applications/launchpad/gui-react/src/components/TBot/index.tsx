import SvgTBotBase from '../../styles/Icons/TBotBase'
import SvgTBotHearts from '../../styles/Icons/TBotHearts'
import SvgTBotHeartsMonero from '../../styles/Icons/TBotHeartsMonero'
import SvgTBotLoading from '../../styles/Icons/TBotLoading'
import SvgTBotRadar from '../../styles/Icons/TBotSearch'

import { TBotProps } from './types'

/**
 * TBot component
 *
 * @prop {TBotType} [type] - TBot variant to render
 * @prop {CSSProperties} [style] - optional TBot additional styling
 *
 * @example
 * <TBot type='hearts' style={{ fontSize: '24px' }} />
 */

const TBot = ({ type = 'base', style }: TBotProps) => {
  const botVariants = {
    base: SvgTBotBase,
    hearts: SvgTBotHearts,
    heartsMonero: SvgTBotHeartsMonero,
    loading: SvgTBotLoading,
    search: SvgTBotRadar,
  }

  const TBotComponent = botVariants[type]

  return <TBotComponent fontSize={74} style={style} data-testid='tbot-cmp' />
}

export default TBot
