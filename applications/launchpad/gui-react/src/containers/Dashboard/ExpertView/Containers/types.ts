import { Service } from '../../../../store/services/types'

type ServiceDto = {
  service: Service
} & any

export type ContainersProps = {
  services: ServiceDto[]
  startService: (service: Service) => void
  stopService: (service: Service) => void
}
