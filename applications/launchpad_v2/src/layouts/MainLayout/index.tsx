import { useLayoutEffect, useRef, useState } from 'react'
import { useSpring } from 'react-spring'
import { useSelector } from 'react-redux'

import { selectExpertView } from '../../store/app/selectors'
import ExpertViewUtils from '../../utils/ExpertViewUtils'
import TitleBar from '../../components/TitleBar'
import DashboardContainer from '../../containers/Dashboard/DashboardContainer'
import ExpertView from '../../containers/Dashboard/ExpertView'
import SettingsContainer from '../../containers/SettingsContainer'

import { MainLayoutProps } from './types'
import {
  ExpertViewBackgroundOverlay,
  ExpertViewDrawer,
  MainContainer,
  MainLayoutContainer,
  ScreenContainer,
} from './styles'

/**
 * Main Layout
 */
const MainLayout = ({ drawerViewWidth = '50%' }: MainLayoutProps) => {
  const mainContainerRef = useRef(null)

  const expertView = useSelector(selectExpertView)

  // Decrease the padding when the main container becomes 'small'
  const [tightSpace, setTightSpace] = useState(false)

  const [settingsOpen, setSettingsOpen] = useState(false)

  const [expertViewSize, invertedExpertViewSize] =
    ExpertViewUtils.convertExpertViewModeToValue(expertView, drawerViewWidth)

  // Decrease the padding in the main container when screen becomes small
  useLayoutEffect(() => {
    const onResize = () => {
      if (mainContainerRef && mainContainerRef.current) {
        // @ts-expect-error: ignore this
        if (mainContainerRef.current.offsetWidth < 960) {
          setTightSpace(true)
        } else {
          setTightSpace(false)
        }
      }
    }

    window.addEventListener('resize', onResize)

    return () => {
      window.removeEventListener('resize', onResize)
    }
  }, [])

  /**
   * Animations
   */
  const mainContainerStyle = useSpring({
    width: expertView === 'open' ? invertedExpertViewSize : '100%',
    paddingLeft: expertView === 'open' || tightSpace ? 40 : 100,
    paddingRight: expertView === 'open' || tightSpace ? 40 : 100,
  })
  const drawerContainerStyle = ExpertViewUtils.useDrawerAnim(expertViewSize)
  const drawerContentStyle = useSpring({
    left: `${invertedExpertViewSize}%`,
    marginRight: expertViewSize === '0%' ? '-100%' : '0%',
    width: expertViewSize === '100%' ? '100%' : drawerViewWidth,
  })

  return (
    <ScreenContainer>
      <TitleBar
        drawerViewWidth={drawerViewWidth}
        openSettings={() => setSettingsOpen(true)}
      />

      <MainLayoutContainer>
        <MainContainer
          ref={mainContainerRef}
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
      <SettingsContainer
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
      />
    </ScreenContainer>
  )
}

export default MainLayout
