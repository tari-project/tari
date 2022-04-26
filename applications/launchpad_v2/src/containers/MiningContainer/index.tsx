import { useDispatch, useSelector } from 'react-redux'
import Switch from '../../components/Switch'

import Text from '../../components/Text'
import SvgSun from '../../styles/Icons/Sun'
import SvgMoon from '../../styles/Icons/Moon'

import { setTheme } from '../../store/app'
import { selectTheme } from '../../store/app/selectors'

/**
 * @TODO move user-facing text to i18n file when implementing
 */

const MiningContainer = () => {
  const dispatch = useDispatch()
  const currentTheme = useSelector(selectTheme)

  return (
    <div>
      <h2>Mining</h2>
      <button onClick={() => dispatch(setTheme('light'))}>
        Set light theme
      </button>
      <button onClick={() => dispatch(setTheme('dark'))}>Set dark theme</button>
      <div>
        <Text>Select theme</Text>
        <Switch
          leftLabel={<SvgSun width='1.4em' height='1.4em' />}
          rightLabel={<SvgMoon width='1.4em' height='1.4em' />}
          value={currentTheme === 'dark'}
          onClick={v => dispatch(setTheme(v ? 'dark' : 'light'))}
        />
      </div>
    </div>
  )
}

export default MiningContainer
