import { ThemeType } from '../../styles/themes/types'
import { Schedule, ScheduleId } from '../../types/general'

export type ExpertViewType = 'hidden' | 'open' | 'fullscreen'
export type ViewType = 'MINING' | 'BASE_NODE' | 'WALLET' | 'ONBOARDING'

export interface AppState {
  expertView: ExpertViewType
  expertSwitchDisabled?: boolean
  view?: ViewType
  theme: ThemeType
  schedules: Record<ScheduleId, Omit<Schedule, 'date'> & { date?: string }>
}
