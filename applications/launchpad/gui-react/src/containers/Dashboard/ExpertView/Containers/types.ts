import { Service } from '../../../../store/services/types'

type ServiceDto = {
  service: Service
  cpu: number
  memory: number
  pending: boolean
  running: boolean
}

export type ContainersProps = {
  services: ServiceDto[]
  startService: (service: Service) => void
  stopService: (service: Service) => void
}
