import { useAppSelector, useAppDispatch } from '../../store/hooks'
import {
  selectState,
  selectPending,
  selectRunning,
  selectHealthy,
  selectUnhealthyContainers,
} from '../../store/baseNode/selectors'
import { setExpertView } from '../../store/app'
import { actions } from '../../store/baseNode'

import BaseNode from './BaseNode'

const BaseNodeContainer = () => {
  const { network } = useAppSelector(selectState)
  const pending = useAppSelector(selectPending)
  const running = useAppSelector(selectRunning)
  const healthy = useAppSelector(selectHealthy)
  const unhealthyContainers = useAppSelector(selectUnhealthyContainers)
  const dispatch = useAppDispatch()

  return (
    <BaseNode
      running={running}
      pending={pending}
      healthy={healthy}
      unhealthyContainers={unhealthyContainers}
      startNode={() => dispatch(actions.startNode())}
      stopNode={() => dispatch(actions.stopNode())}
      tariNetwork={network}
      setTariNetwork={network => dispatch(actions.setTariNetwork(network))}
      openExpertView={() => dispatch(setExpertView('open'))}
    />
  )
}

export default BaseNodeContainer
