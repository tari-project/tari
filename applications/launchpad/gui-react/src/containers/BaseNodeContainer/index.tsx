import { useAppSelector, useAppDispatch } from '../../store/hooks'
import {
  selectState,
  selectContainerStatuses,
} from '../../store/baseNode/selectors'
import { actions } from '../../store/baseNode'

import BaseNode from './BaseNode'

const BaseNodeContainer = () => {
  const { network } = useAppSelector(selectState)
  const containers = useAppSelector(selectContainerStatuses)
  const dispatch = useAppDispatch()

  return (
    <BaseNode
      containers={containers}
      startNode={() => dispatch(actions.startNode())}
      stopNode={() => dispatch(actions.stopNode())}
      tariNetwork={network}
      setTariNetwork={network => dispatch(actions.setTariNetwork(network))}
    />
  )
}

export default BaseNodeContainer
