import OnboardingContainer from '../../containers/Onboarding'
import ExpertView from '../../containers/Onboarding/ExpertView'
import MainLayout from '../../layouts/MainLayout'

/**
 * Onboarding
 */
const Onboarding = () => {
  return (
    <MainLayout
      ChildrenComponent={OnboardingContainer}
      ExpertViewComponent={ExpertView}
      drawerViewWidth='40%'
      titleBarProps={{
        hideSettingsButton: true,
      }}
    />
  )
}

export default Onboarding
