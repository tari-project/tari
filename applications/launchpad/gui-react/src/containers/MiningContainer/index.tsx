import { useState, useRef } from 'react'

import Switch from '../../components/Switch'

import Text from '../../components/Text'
import SvgSun from '../../styles/Icons/Sun'
import SvgMoon from '../../styles/Icons/Moon'

import MiningHeaderTip from './MiningHeaderTip'
import MiningViewActions from './MiningViewActions'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { setTheme } from '../../store/app'
import { selectTheme } from '../../store/app/selectors'

import { NodesContainer } from './styles'
import MiningBoxTari from './MiningBoxTari'
import MiningBoxMerged from './MiningBoxMerged'
import Scheduling from './Scheduling'
import Statistics from './Statistics'

/**
 * The Mining dashboard
 */
const MiningContainer = () => {
  const dispatch = useAppDispatch()
  const currentTheme = useAppSelector(selectTheme)
  const [schedulingOpen, setSchedulingOpen] = useState(false)
  const [statisticsOpen, setStatisticsOpen] = useState(false)
  const anchorElRef = useRef<HTMLAnchorElement>(null)

  return (
    <div>
      <MiningHeaderTip />

      <NodesContainer>
        <MiningBoxTari />
        <MiningBoxMerged />
      </NodesContainer>

      <MiningViewActions
        openScheduling={() => setSchedulingOpen(true)}
        toggleStatistics={() => {
          setStatisticsOpen(o => !o)
        }}
      />
      <Scheduling
        open={schedulingOpen}
        onClose={() => setSchedulingOpen(false)}
      />
      <Statistics
        open={statisticsOpen}
        onClose={() => setStatisticsOpen(false)}
        onReady={() => {
          anchorElRef.current?.scrollIntoView({
            behavior: 'smooth',
            block: 'end',
          })
        }}
      />
      <a id='anchorForScroll' ref={anchorElRef} />

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
    </div>
  )
}

export default MiningContainer
