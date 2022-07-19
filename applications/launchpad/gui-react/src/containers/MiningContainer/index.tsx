import { useState, useRef } from 'react'

import MiningHeaderTip from './MiningHeaderTip'
import MiningViewActions from './MiningViewActions'

import { useAppDispatch } from '../../store/hooks'
import { actions as settingsActions } from '../../store/settings'

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
        openSettings={() => dispatch(settingsActions.open({}))}
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
    </div>
  )
}

export default MiningContainer
