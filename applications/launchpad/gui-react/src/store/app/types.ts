import { ThemeType } from '../../styles/themes/types'
import { Schedule, ScheduleId } from '../../types/general'

export type ExpertViewType = 'hidden' | 'open' | 'fullscreen'
export type ViewType = 'MINING' | 'BASE_NODE' | 'WALLET' | 'ONBOARDING'

export enum DockerImagePullStatus {
  Waiting = 'waiting',
  Pulling = 'pulling',
  Ready = 'ready',
}

export type DockerImage = {
  imageName: string
  displayName: string
  dockerImage: string
  latest: boolean
  pending?: boolean
  error?: string
  progress?: number
  status?: DockerImagePullStatus
}

export interface AppState {
  expertView: ExpertViewType
  expertSwitchDisabled?: boolean
  view?: ViewType
  theme: ThemeType
  schedules: Record<ScheduleId, Omit<Schedule, 'date'> & { date?: string }>
  onboardingComplete?: boolean
  dockerImages: {
    loaded: boolean
    images: DockerImage[]
  }
}
