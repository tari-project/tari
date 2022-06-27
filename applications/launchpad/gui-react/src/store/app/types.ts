import { ThemeType } from '../../styles/themes/types'
import { Schedule, ScheduleId } from '../../types/general'

export type ExpertViewType = 'hidden' | 'open' | 'fullscreen'
export type ViewType = 'MINING' | 'BASE_NODE' | 'WALLET' | 'ONBOARDING'

export enum OnboardingCheckpoints {
  DOCKER_INSTALL = 'docker_install',
  DOCKER_IMAGES_DOWNLOAD = 'docker_images_download',
  BLOCKCHAIN_SYNC = 'blockchain_sync',
}

export interface AppState {
  expertView: ExpertViewType
  expertSwitchDisabled?: boolean
  view?: ViewType
  theme: ThemeType
  schedules: Record<ScheduleId, Omit<Schedule, 'date'> & { date?: string }>
  onboardingComplete?: boolean
  onboardingCheckpoint?: OnboardingCheckpoints
}
