import { RootState } from '../'
import { Container } from '../containers/types'
import { selectContainerStatus } from '../containers/selectors'
import type { Network } from '../../containers/BaseNodeContainer/types'

import { BaseNodeState } from './types'

export const selectState = (state: RootState): BaseNodeState => ({
  network: state.baseNode.network as Network,
})

const requiredContainers = [Container.Tor, Container.BaseNode]
export const selectContainerStatuses = (rootState: RootState) =>
  requiredContainers.map(containerType =>
    selectContainerStatus(containerType)(rootState),
  )

export const selectStatus = selectContainerStatus(Container.BaseNode)
