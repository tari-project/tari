import { useSpring } from 'react-spring'
import { useSelector } from 'react-redux'

import { MainLayoutProps } from './types'
import {
  ExpertViewBackgroundOverlay,
  ExpertViewDrawer,
  MainContainer,
  MainLayoutContainer,
  ScreenContainer,
} from './styles'

import { selectExpertView } from '../../store/app/selectors'
import ExpertViewUtils from '../../utils/ExpertViewUtils'
import TitleBar from '../../components/TitleBar'
import DashboardContainer from '../../containers/Dashboard/DashboardContainer'
import ExpertView from '../../containers/Dashboard/ExpertView'

/**
 * Main Layout
 */
const MainLayout = ({ drawerViewWidth = '50%' }: MainLayoutProps) => {
  const expertView = useSelector(selectExpertView)

  const [expertViewSize, invertedExpertViewSize] =
    ExpertViewUtils.convertExpertViewModeToValue(expertView, drawerViewWidth)

  /**
   * Animations
   */
  const mainContainerStyle = useSpring({
    width: expertView === 'open' ? invertedExpertViewSize : '100%',
  })
  const drawerContainerStyle = ExpertViewUtils.useDrawerAnim(expertViewSize)
  const drawerContentStyle = useSpring({
    left: `${invertedExpertViewSize}%`,
    marginRight: expertViewSize === '0%' ? '-100%' : '0%',
    width: expertViewSize === '100%' ? '100%' : drawerViewWidth,
  })

  return (
    <ScreenContainer>
      <TitleBar drawerViewWidth={drawerViewWidth} />

      <MainLayoutContainer>
        <MainContainer
          style={{
            ...mainContainerStyle,
          }}
        >
          <DashboardContainer />
        </MainContainer>

        {/* Background overlay: */}
        <ExpertViewBackgroundOverlay
          style={{
            borderRadius: expertViewSize === '100%' ? 10 : 0,
            ...drawerContainerStyle,
          }}
        />

        {/* Actual content: */}
        <ExpertViewDrawer
          style={{
            borderRadius: expertViewSize === '100%' ? 10 : 0,
            ...drawerContentStyle,
          }}
        >
          <ExpertView />
        </ExpertViewDrawer>
      </MainLayoutContainer>
    </ScreenContainer>
  )
}

export default MainLayout
