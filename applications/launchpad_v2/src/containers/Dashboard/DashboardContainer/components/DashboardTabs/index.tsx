import { useEffect, useState } from 'react'
import { useDispatch, useSelector } from 'react-redux'

import Loading from '../../../../../components/Loading'
import Tabs from '../../../../../components/Tabs'
import Tag from '../../../../../components/Tag'

import { setPage } from '../../../../../store/app'
import { ViewType } from '../../../../../store/app/types'
import { selectView } from '../../../../../store/app/selectors'
import { selectState as selectBaseNodeState } from '../../../../../store/baseNode/selectors'
import { BaseNodeState } from '../../../../../store/baseNode/types'
import { selectState as selectWalletState } from '../../../../../store/wallet/selectors'
import { WalletState } from '../../../../../store/wallet/types'

import t from '../../../../../locales'

import {
  StyledTabContent,
  TabMainText,
  LoadingWrapper,
  TabTagSubText,
} from './styles'

/**
 * Helper unifying the way how the content of each tab tile is composed.
 */
const TabContent = ({
  text,
  running,
  pending,
  tagSubText,
}: {
  text: string
  running?: boolean
  pending?: boolean
  tagSubText?: string
}) => {
  return (
    <StyledTabContent>
      <TabMainText spacingRight={running || pending ? true : false}>
        {text}
      </TabMainText>
      {running && !pending ? (
        <Tag variant='small' type='running'>
          <span>{t.common.adjectives.running}</span>
          {tagSubText ? (
            <>
              {' '}
              <TabTagSubText>{tagSubText}</TabTagSubText>
            </>
          ) : null}
        </Tag>
      ) : null}
      {pending ? (
        <LoadingWrapper>
          <Loading loading={true} size='12px' />
        </LoadingWrapper>
      ) : null}
    </StyledTabContent>
  )
}

/**
 * Helper composing all dashboard tabs.
 */
const composeNodeTabs = ({
  miningNodeState,
  baseNodeState,
  walletState,
}: {
  miningNodeState?: unknown
  baseNodeState?: BaseNodeState
  walletState?: WalletState
}) => {
  const miningContent = <TabContent text={t.common.nouns.mining} />

  const baseNodeContent = (
    <TabContent
      text={t.common.nouns.baseNode}
      running={baseNodeState?.running}
      pending={baseNodeState?.pending}
      tagSubText={
        baseNodeState?.network
          ? baseNodeState.network[0].toUpperCase() +
            baseNodeState.network.substring(1)
          : undefined
      }
    />
  )

  const walletContent = (
    <TabContent
      text={t.common.nouns.wallet}
      running={walletState?.running}
      pending={walletState?.pending}
    />
  )

  return [
    {
      id: 'MINING',
      content: miningContent,
    },
    {
      id: 'BASE_NODE',
      content: baseNodeContent,
    },
    {
      id: 'WALLET',
      content: walletContent,
    },
  ]
}

/**
 * Renders Dasboard tabs
 */
const DashboardTabs = () => {
  const dispatch = useDispatch()

  const currentPage = useSelector(selectView)
  const baseNodeState = useSelector(selectBaseNodeState)
  const walletState = useSelector(selectWalletState)

  const [tabs, setTabs] = useState(
    composeNodeTabs({
      miningNodeState: undefined,
      baseNodeState,
      walletState,
    }),
  )

  useEffect(() => {
    setTabs(
      composeNodeTabs({
        miningNodeState: undefined,
        baseNodeState,
        walletState,
      }),
    )
  }, [baseNodeState, currentPage])

  const setPageTab = (tabId: string) => {
    dispatch(setPage(tabId as ViewType))
  }

  return (
    <Tabs
      tabs={tabs}
      selected={currentPage || 'MINING'}
      onSelect={setPageTab}
    />
  )
}

export default DashboardTabs
