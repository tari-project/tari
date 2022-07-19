import { useState } from 'react'

import { useAppDispatch, useAppSelector } from '../../../store/hooks'
import { setExpertView } from '../../../store/app'
import { selectExpertView } from '../../../store/app/selectors'
import Tabs from '../../../components/Tabs'
import Button from '../../../components/Button'
import Text from '../../../components/Text'
import TabContent from '../../../components/TabContent'
import ExpandIcon from '../../../styles/Icons/Monitor'
import CollapseIcon from '../../../styles/Icons/Grid'
import { MainContainer } from '../../../layouts/MainLayout/styles'
import t from '../../../locales'

import Containers from './Containers'
import Performance from './Performance'
import Logs from './Logs'
import {
  TabsContainer,
  PageContentContainer,
  ScrollablePageContentContainer,
} from './styles'

const ExpertView = () => {
  const dispatch = useAppDispatch()
  const expertView = useAppSelector(selectExpertView)
  const [selectedTab, setTab] = useState('CONTAINERS')

  const isFullscreen = expertView === 'fullscreen'

  const tabs = [
    {
      id: 'PERFORMANCE',
      content: <TabContent text={t.common.nouns.performance} />,
    },
    {
      id: 'CONTAINERS',
      content: <TabContent text={t.common.nouns.containers} />,
    },
    {
      id: 'LOGS',
      content: <TabContent text={t.common.nouns.logs} />,
    },
  ]

  const renderPage = () => {
    if (expertView === 'hidden') {
      return null
    }

    switch (selectedTab) {
      case 'PERFORMANCE':
        return (
          <ScrollablePageContentContainer>
            <Performance />
          </ScrollablePageContentContainer>
        )
      case 'CONTAINERS':
        return (
          <ScrollablePageContentContainer>
            <Containers />
          </ScrollablePageContentContainer>
        )
      case 'LOGS':
        return (
          <PageContentContainer>
            <Logs />
          </PageContentContainer>
        )
      default:
        return null
    }
  }

  return (
    <MainContainer
      style={{
        height: '100%',
      }}
    >
      <TabsContainer>
        <Tabs tabs={tabs} selected={selectedTab} onSelect={setTab} inverted />
        {!isFullscreen && (
          <Button
            variant='text'
            autosizeIcons={false}
            leftIcon={<ExpandIcon width='24px' height='24px' />}
            style={{ paddingRight: 0, paddingLeft: 0 }}
            onClick={() => dispatch(setExpertView('fullscreen'))}
          >
            <Text type='smallMedium'>{t.expertView.fullscreen.open}</Text>
          </Button>
        )}
        {isFullscreen && (
          <Button
            variant='text'
            autosizeIcons={false}
            leftIcon={<CollapseIcon width='24px' height='24px' />}
            style={{ paddingRight: 0, paddingLeft: 0 }}
            onClick={() => dispatch(setExpertView('open'))}
          >
            <Text type='smallMedium'>{t.expertView.fullscreen.close}</Text>
          </Button>
        )}
      </TabsContainer>
      {renderPage()}
    </MainContainer>
  )
}

export default ExpertView
