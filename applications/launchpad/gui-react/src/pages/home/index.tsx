import DashboardContainer from '../../containers/Dashboard/DashboardContainer'
import MainLayout from '../../layouts/MainLayout'

/**
 * Home page: '/'
 */
const HomePage = () => {
  return <MainLayout ChildrenComponent={DashboardContainer} />
}

export default HomePage
