import { useMemo } from 'react'

import { useAppSelector, useAppDispatch } from '../../../../store/hooks'
import { selectContainersStatuses } from '../../../../store/containers/selectors'
import { Container, ContainerId } from '../../../../store/containers/types'
import { actions } from '../../../../store/containers'

import Containers from './Containers'

const ContainersContainer = () => {
  const dispatch = useAppDispatch()
  const containerStatuses = useAppSelector(selectContainersStatuses)
  const services = useMemo(
    () =>
      containerStatuses.map(({ service, status }) => ({
        id: status.id,
        service: service as Container,
        cpu: status.stats.cpu,
        memory: status.stats.memory,
        pending: status.pending,
        running: status.running,
      })),
    [containerStatuses],
  )

  const start = (container: Container) => dispatch(actions.start(container))
  const stop = (containerId: ContainerId) => dispatch(actions.stop(containerId))

  return <Containers containers={services} stop={stop} start={start} />
}

export default ContainersContainer
