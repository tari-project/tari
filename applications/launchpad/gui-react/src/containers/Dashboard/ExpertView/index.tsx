import { useState } from 'react'

import { useAppDispatch, useAppSelector } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { selectExpertView } from '../../../store/app/selectors'
import Tabs from '../../../components/Tabs'
import Box from '../../../components/Box'
import TabContent from '../../../components/TabContent'

const ExpertView = () => {
  const dispatch = useAppDispatch()
  const expertView = useAppSelector(selectExpertView)
  const [selectedTab, setTab] = useState('CONTAINERS')

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
    <Box
      border={false}
      style={{
        background: 'none',
        width: '100%',
        borderRadius: 0,
      }}
    >
      <Tabs tabs={tabs} selected={selectedTab} onSelect={setTab} inverted />
      <Box
        border={false}
        style={{
          background: 'none',
          width: '100%',
          borderRadius: 0,
        }}
      >
        {renderPage()}
      </Box>
    </Box>
  )
}

export default ExpertView
