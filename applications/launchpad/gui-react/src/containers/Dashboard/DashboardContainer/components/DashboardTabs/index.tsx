import { useMemo } from 'react'
import { useDispatch, useSelector } from 'react-redux'

import Tabs from '../../../../../components/Tabs'
import TabContent from '../../../../../components/TabContent'

import { setPage } from '../../../../../store/app'
import { ViewType } from '../../../../../store/app/types'
import { selectView } from '../../../../../store/app/selectors'
import {
  selectPending as selectBaseNodePending,
  selectRunning as selectBaseNodeRunning,
} from '../../../../../store/baseNode/selectors'
import { selectState as selectWalletState } from '../../../../../store/wallet/selectors'
import { WalletState } from '../../../../../store/wallet/types'

import t from '../../../../../locales'

/**
 * Helper composing all dashboard tabs.
 */
const composeNodeTabs = ({
  miningNodeState,
  baseNodeState,
  walletState,
}: {
  miningNodeState?: unknown
  baseNodeState: { pending: boolean; running: boolean }
  walletState?: WalletState
}) => {
  const miningContent = <TabContent text={t.common.nouns.mining} />

  const baseNodeContent = (
    <TabContent
      text={t.common.nouns.baseNode}
      pending={baseNodeState.pending}
      running={baseNodeState.running}
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
  const baseNodePending = useSelector(selectBaseNodePending)
  const baseNodeRunning = useSelector(selectBaseNodeRunning)
  const walletState = useSelector(selectWalletState)

  const tabs = useMemo(
    () =>
      composeNodeTabs({
        miningNodeState: undefined,
        baseNodeState: { pending: baseNodePending, running: baseNodeRunning },
        walletState,
      }),
    [walletState, baseNodePending],
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
