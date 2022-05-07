import { useMemo } from 'react'

import { useAppSelector, useAppDispatch } from '../../../../store/hooks'
import { selectState as selectServicesState } from '../../../../store/services/selectors'
import { Service } from '../../../../store/services/types'
import { actions } from '../../../../store/services'
import t from '../../../../locales'

import Containers from './Containers'

const ContainersContainer = () => {
  const dispatch = useAppDispatch()
  const { servicesStatus } = useAppSelector(selectServicesState)
  const services = useMemo(
    () =>
      Object.entries(servicesStatus).map(([service, status]) => ({
        id: status.id,
        service,
        name: t.common.services[service],
        cpu: 7,
        memory: '12 MB',
        pending: status.pending,
        running: status.running,
      })),
    [servicesStatus],
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
