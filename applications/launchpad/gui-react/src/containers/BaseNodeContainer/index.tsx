import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { selectState, selectStatus } from '../../store/baseNode/selectors'
import { actions } from '../../store/baseNode'

import BaseNode from './BaseNode'

const BaseNodeContainer = () => {
  const { network } = useAppSelector(selectState)
  const { running, pending } = useAppSelector(selectStatus)
  const dispatch = useAppDispatch()

  return (
    <BaseNode
      running={running}
      pending={pending}
      startNode={() => dispatch(actions.startNode())}
      stopNode={() => dispatch(actions.stopNode())}
      tariNetwork={network}
      setTariNetwork={network => dispatch(actions.setTariNetwork(network))}
    />
  )
}

export default BaseNodeContainer
