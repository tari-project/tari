import { useDispatch, useSelector } from 'react-redux'

import { DashboardContent, DashboardLayout } from './styles'

import MiningContainer from '../../MiningContainer'
import BaseNodeContainer from '../../BaseNodeContainer'
import WalletContainer from '../../WalletContainer'

import Footer from '../../../components/Footer'
import Tabs from '../../../components/Tabs'

import { setPage } from '../../../store/app'
import { ViewType } from '../../../store/app/types'
import { selectView } from '../../../store/app/selectors'

/**
 * @TODO move user-facing text to i18n file when implementing
 */

/**
 * Dashboard view containing three main tabs: Mining, Wallet and BaseNode
 */
const DashboardContainer = () => {
  const dispatch = useDispatch()

  const currentPage = useSelector(selectView)

  const pageTabs = [
    {
      id: 'MINING',
      content: <span>Mining</span>,
    },
    {
      id: 'BASE_NODE',
      content: <span>Base Node</span>,
    },
    {
      id: 'WALLET',
      content: <span>Wallet</span>,
    },
  ]

  const setPageTab = (tabId: string) => {
    dispatch(setPage(tabId as ViewType))
  }

  const renderPage = () => {
    switch (currentPage) {
      case 'MINING':
        return <MiningContainer />
      case 'BASE_NODE':
        return <BaseNodeContainer />
      case 'WALLET':
        return <WalletContainer />
      default:
        return null
    }
  }

  return (
    <DashboardLayout>
      <DashboardContent>
        <Tabs
          tabs={pageTabs}
          selected={currentPage || 'MINING'}
          onSelect={setPageTab}
        />
        {renderPage()}
      </DashboardContent>

      <Footer />
    </DashboardLayout>
  )
}

export default DashboardContainer
