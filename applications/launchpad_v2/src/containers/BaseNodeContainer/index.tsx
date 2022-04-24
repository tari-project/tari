import { useAppSelector, useAppDispatch } from '../../store/hooks'
import BaseNode from './BaseNode'
import { selectState } from './slice/selectors'
import { actions } from './slice'

const BaseNodeContainer = () => {
  const { network, running, pending } = useAppSelector(selectState)
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
