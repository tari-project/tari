import { useMemo } from 'react'

import { useAppSelector, useAppDispatch } from '../../../../store/hooks'
import { selectAllServicesStatuses } from '../../../../store/services/selectors'
import { Service } from '../../../../store/services/types'
import { actions } from '../../../../store/services'

import Containers from './Containers'

const ContainersContainer = () => {
  const dispatch = useAppDispatch()
  const allServicesStatuses = useAppSelector(selectAllServicesStatuses)
  const services = useMemo(
    () =>
      allServicesStatuses.map(({ service, status }) => ({
        service: service as Service,
        cpu: status.stats.cpu,
        memory: status.stats.memory,
        pending: status.pending,
        running: status.running,
      })),
    [allServicesStatuses],
  )

  const startService = (service: Service) => dispatch(actions.start(service))
  const stopService = (service: Service) => dispatch(actions.stop(service))

  return (
    <Containers
      services={services}
      stopService={stopService}
      startService={startService}
    />
  )
}

export default ContainersContainer
