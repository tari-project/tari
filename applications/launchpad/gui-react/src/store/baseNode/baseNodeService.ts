import { invoke } from '@tauri-apps/api/tauri'
import { BaseNodeIdentityDto } from './types'

export const getIdentity: () => Promise<BaseNodeIdentityDto> = () =>
  invoke<BaseNodeIdentityDto>('node_identity')
