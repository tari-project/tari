import SvgTBotBase from '../../styles/Icons/TBotBase'
import SvgTBotHearts from '../../styles/Icons/TBotHearts'
import SvgTBotHeartsMonero from '../../styles/Icons/TBotHeartsMonero'
import SvgTBotLoading from '../../styles/Icons/TBotLoading'
import SvgTBotRadar from '../../styles/Icons/TBotRadar'

import { TBotProps } from './types'

const TBot = ({ type, size, style }: TBotProps) => {
  const botVariants = {
    base: SvgTBotBase,
    hearts: SvgTBotHearts,
    heartsMonero: SvgTBotHeartsMonero,
    loading: SvgTBotLoading,
    radar: SvgTBotRadar,
  }

  const TBotComponent = botVariants[type]

  return <TBotComponent fontSize={size} style={style} />
}

export default TBot
