import { RootState } from '../'
import { Service } from '../services/types'
import { selectServiceStatus } from '../services/selectors'
import type { Network } from '../../containers/BaseNodeContainer/types'

import { BaseNodeState } from './types'

export const selectState = (state: RootState): BaseNodeState => ({
  network: state.baseNode.network as Network,
})

export const selectStatus = selectServiceStatus(Service.BaseNode)
