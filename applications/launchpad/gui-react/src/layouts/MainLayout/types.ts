import { TitleBarProps } from '../../components/TitleBar/types'
import DashboardContainer from '../../containers/Dashboard/DashboardContainer'
import ExpertView from '../../containers/Dashboard/ExpertView'
import OnboardingContainer from '../../containers/Onboarding'

export interface MainLayoutProps {
  drawerViewWidth?: string
  ChildrenComponent: typeof DashboardContainer | typeof OnboardingContainer
  ExpertViewComponent?: typeof ExpertView
  titleBarProps?: TitleBarProps
}
