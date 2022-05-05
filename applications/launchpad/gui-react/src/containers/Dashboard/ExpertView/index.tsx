import { useState } from 'react'

import { useAppDispatch, useAppSelector } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { selectExpertView } from '../../../store/app/selectors'
import Tabs from '../../../components/Tabs'
import Button from '../../../components/Button'
import TabContent from '../../../components/TabContent'
import ExpandIcon from '../../../styles/Icons/Monitor'
import CollapseIcon from '../../../styles/Icons/Grid'

import { TabsContainer, ExpertBox } from './styles'

const ExpertView = () => {
  const dispatch = useAppDispatch()
  const expertView = useAppSelector(selectExpertView)
  const [selectedTab, setTab] = useState('CONTAINERS')

  const isFullscreen = expertView === 'fullscreen'

  const tabs = [
    {
      id: 'PERFORMANCE',
      content: <TabContent text='Performance' />,
    },
    {
      id: 'CONTAINERS',
      content: <TabContent text='Containers' />,
    },
    {
      id: 'LOGS',
      content: <TabContent text='Logs' />,
    },
  ]

  const renderPage = () => {
    switch (selectedTab) {
      case 'PERFORMANCE':
        return <p style={{ color: 'white' }}>performance tab</p>
      case 'CONTAINERS':
        return <p style={{ color: 'white' }}>containers tab</p>
      case 'LOGS':
        return <p style={{ color: 'white' }}>logs tab</p>
      default:
        return null
    }
  }

  return (
    <ExpertBox>
      <TabsContainer>
        <Tabs tabs={tabs} selected={selectedTab} onSelect={setTab} inverted />
        {!isFullscreen && (
          <Button
            variant='text'
            leftIcon={<ExpandIcon width='20px' height='20px' />}
            style={{ paddingRight: 0, paddingLeft: 0 }}
            onClick={() => dispatch(setExpertView('fullscreen'))}
          >
            Open full screen
          </Button>
        )}
        {isFullscreen && (
          <Button
            variant='text'
            leftIcon={<CollapseIcon width='20px' height='20px' />}
            style={{ paddingRight: 0, paddingLeft: 0 }}
            onClick={() => dispatch(setExpertView('open'))}
          >
            Close full screen
          </Button>
        )}
      </TabsContainer>
      <ExpertBox>{renderPage()}</ExpertBox>
    </ExpertBox>
  )
}

export default ExpertView
