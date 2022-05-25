import { useMemo } from 'react'
import { useDispatch } from 'react-redux'

import Tabs from '../../../../../components/Tabs'
import TabContent from '../../../../../components/TabContent'

import { setPage } from '../../../../../store/app'
import { ViewType } from '../../../../../store/app/types'
import { selectView } from '../../../../../store/app/selectors'
import {
  selectNetwork,
  selectPending as selectBaseNodePending,
  selectRunning as selectBaseNodeRunning,
} from '../../../../../store/baseNode/selectors'
import { selectState as selectWalletState } from '../../../../../store/wallet/selectors'
import t from '../../../../../locales'
import {
  selectIsMiningPending,
  selectIsMiningRunning,
} from '../../../../../store/mining/selectors'
import { useAppSelector } from '../../../../../store/hooks'

/**
 * Helper composing all dashboard tabs.
 */
const composeNodeTabs = ({
  miningState,
  baseNodeState,
  walletState,
}: {
  miningState: { pending: boolean; running: boolean }
  baseNodeState: { pending: boolean; running: boolean; network?: string }
  walletState: { pending: boolean; running: boolean }
}) => {
  const miningContent = (
    <TabContent
      text={t.common.nouns.mining}
      pending={miningState.pending}
      running={miningState.running}
    />
  )

  const baseNodeContent = (
    <TabContent
      text={t.common.nouns.baseNode}
      pending={baseNodeState.pending}
      running={baseNodeState.running}
      tagSubText={
        baseNodeState.running && baseNodeState.network
          ? baseNodeState.network
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

  const currentPage = useAppSelector(selectView)
  const baseNodePending = useAppSelector(selectBaseNodePending)
  const baseNodeRunning = useAppSelector(selectBaseNodeRunning)
  const baseNodeNetwork = useAppSelector(selectNetwork)
  const walletState = useAppSelector(selectWalletState)
  const miningRunning = useAppSelector(selectIsMiningRunning)
  const miningPending = useAppSelector(selectIsMiningPending)

  const tabs = useMemo(
    () =>
      composeNodeTabs({
        miningState: { pending: miningPending, running: miningRunning },
        baseNodeState: {
          pending: baseNodePending,
          running: baseNodeRunning,
          network: baseNodeNetwork,
        },
        walletState: {
          pending: walletState.pending,
          running: walletState.running,
        },
      }),
    [
      walletState,
      baseNodePending,
      miningPending,
      miningRunning,
      baseNodeNetwork,
    ],
  )

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
