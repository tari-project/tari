import { Container, ContainerId } from '../../../../store/containers/types'

type ServiceDto = {
  id: ContainerId
  service: Container
  cpu: number
  memory: number
  pending: boolean
  running: boolean
}

export type ContainersProps = {
  containers: ServiceDto[]
  start: (container: Container) => void
  stop: (container: ContainerId) => void
}
