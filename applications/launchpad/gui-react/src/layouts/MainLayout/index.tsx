import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { useTheme } from 'styled-components'
import { useSpring } from 'react-spring'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { setExpertSwitchDisabled } from '../../store/app'
import { selectExpertView } from '../../store/app/selectors'
import ExpertViewUtils from '../../utils/ExpertViewUtils'
import TitleBar from '../../components/TitleBar'
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
const MainLayout = ({
  drawerViewWidth = '50%',
  ChildrenComponent,
  ExpertViewComponent = ExpertView,
  titleBarProps,
}: MainLayoutProps) => {
  const mainContainerRef = useRef(null)
  const theme = useTheme()
  const dispatch = useAppDispatch()

  const expertView = useAppSelector(selectExpertView)

  // Decrease the padding when the main container becomes 'small'
  const [tightSpace, setTightSpace] = useState(false)

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

  useEffect(() => {
    dispatch(setExpertSwitchDisabled(false))
  })

  /**
   * Animations
   */
  const mainContainerStyle = useSpring({
    width: expertView === 'open' ? invertedExpertViewSize : '100%',
  })
  const dashboardContainerStyle = useSpring({
    paddingLeft:
      expertView === 'open' || tightSpace
        ? theme.spacing(1.5)
        : theme.spacing(4),
    paddingRight:
      expertView === 'open' || tightSpace
        ? theme.spacing(1.5)
        : theme.spacing(4),
  })
  const drawerContainerStyle = ExpertViewUtils.useDrawerAnim(expertViewSize)
  const drawerContentStyle = useSpring({
    left: `${invertedExpertViewSize}%`,
    marginRight: expertViewSize === '0%' ? '-100%' : '0%',
    width: expertViewSize === '100%' ? '100%' : drawerViewWidth,
  })

  return (
    <ScreenContainer>
      <TitleBar drawerViewWidth={drawerViewWidth} {...titleBarProps} />

      <MainLayoutContainer>
        <MainContainer
          ref={mainContainerRef}
          style={{
            ...mainContainerStyle,
          }}
        >
          <ChildrenComponent style={{ ...dashboardContainerStyle }} />
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
          <ExpertViewComponent />
        </ExpertViewDrawer>
      </MainLayoutContainer>
      <SettingsContainer />
    </ScreenContainer>
  )
}

export default MainLayout
