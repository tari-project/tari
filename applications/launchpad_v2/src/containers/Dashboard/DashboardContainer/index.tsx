import { useSelector } from 'react-redux'

import { DashboardContent, DashboardLayout } from './styles'

import MiningContainer from '../../MiningContainer'
import BaseNodeContainer from '../../BaseNodeContainer'
import WalletContainer from '../../WalletContainer'

import DashboardTabs from './components/DashboardTabs'
import Footer from '../../../components/Footer'

import { selectView } from '../../../store/app/selectors'

/**
 * Dashboard view containing three main tabs: Mining, Wallet and BaseNode
 */
const DashboardContainer = () => {
  const currentPage = useSelector(selectView)

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
        <DashboardTabs />
        {renderPage()}
      </DashboardContent>

      <Footer />
    </DashboardLayout>
  )
}

export default DashboardContainer
