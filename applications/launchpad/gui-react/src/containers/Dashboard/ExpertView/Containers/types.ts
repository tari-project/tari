import { Container, ContainerId } from '../../../../store/containers/types'

type ContainerDto = {
  id: ContainerId
  container: Container
  cpu: number
  memory: number
  pending: boolean
  running: boolean
}

export type ContainersProps = {
  containers: ContainerDto[]
  start: (container: Container) => void
  stop: (container: ContainerId) => void
}
