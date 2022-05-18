import { useState } from 'react'

import Switch from '../../components/Switch'

import Text from '../../components/Text'
import SvgSun from '../../styles/Icons/Sun'
import SvgMoon from '../../styles/Icons/Moon'

import MiningHeaderTip from './MiningHeaderTip'
import MiningViewActions from './MiningViewActions'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { setTheme } from '../../store/app'
import { selectTheme } from '../../store/app/selectors'

import { actions } from '../../store/wallet'
import Button from '../../components/Button'
import SvgArrowLeft1 from '../../styles/Icons/ArrowLeft1'
import SvgWallet from '../../styles/Icons/Wallet'
import SvgSetting from '../../styles/Icons/Setting'

import { NodesContainer } from './styles'
import MiningBoxTari from './MiningBoxTari'
import MiningBoxMerged from './MiningBoxMerged'
import Scheduling from './Scheduling'

/**
 * The Mining dashboard
 */
const MiningContainer = () => {
  const dispatch = useAppDispatch()
  const currentTheme = useAppSelector(selectTheme)
  const [schedulingOpen, setSchedulingOpen] = useState(false)

  return (
    <div>
      <MiningHeaderTip />

      <NodesContainer>
        <MiningBoxTari />
        <MiningBoxMerged />
      </NodesContainer>

      <MiningViewActions openScheduling={() => setSchedulingOpen(true)} />

      <button onClick={() => dispatch(actions.unlockWallet('pass'))}>
        Set pass
      </button>

      <button onClick={() => dispatch(actions.unlockWallet(''))}>
        Clear pass
      </button>

      <div style={{ marginTop: 80 }}>
        <button onClick={() => dispatch(setTheme('light'))}>
          Set light theme
        </button>
        <button onClick={() => dispatch(setTheme('dark'))}>
          Set dark theme
        </button>
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
      <Scheduling
        open={schedulingOpen}
        onClose={() => setSchedulingOpen(false)}
      />
    </div>
  )
}

export default MiningContainer
