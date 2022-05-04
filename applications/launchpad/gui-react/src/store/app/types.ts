import { ThemeType } from '../../styles/themes/types'

export type ExpertViewType = 'hidden' | 'open' | 'fullscreen'
export type ViewType = 'MINING' | 'BASE_NODE' | 'WALLET' | 'ONBOARDING'

export interface AppState {
  expertView: ExpertViewType
  view?: ViewType
  theme: ThemeType
}
