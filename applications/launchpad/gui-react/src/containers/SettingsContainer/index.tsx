import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { actions } from '../../store/settings'
import {
  selectSettingsOpen,
  selectActiveSettings,
} from '../../store/settings/selectors'

import SettingsComponent from './SettingsComponent'

const SettingsContainer = () => {
  const dispatch = useAppDispatch()
  const settingsOpen = useAppSelector(selectSettingsOpen)
  const activeSettings = useAppSelector(selectActiveSettings)

  return (
    <SettingsComponent
      open={settingsOpen}
      onClose={() => dispatch(actions.close())}
      activeSettings={activeSettings}
      goToSettings={settingsPage => dispatch(actions.goTo(settingsPage))}
    />
  )
}

export default SettingsContainer
