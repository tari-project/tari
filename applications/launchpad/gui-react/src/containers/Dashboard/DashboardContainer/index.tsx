import { useSelector } from 'react-redux'
import { CSSProperties } from 'styled-components'
import { SpringValue } from 'react-spring'

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
const DashboardContainer = ({
  style,
}: {
  style?:
    | CSSProperties
    | Record<string, SpringValue<number>>
    | Record<string, SpringValue<string>>
}) => {
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
    <DashboardLayout style={style}>
      <DashboardContent>
        <DashboardTabs />
        {renderPage()}
      </DashboardContent>

      <Footer />
    </DashboardLayout>
  )
}

export default DashboardContainer
