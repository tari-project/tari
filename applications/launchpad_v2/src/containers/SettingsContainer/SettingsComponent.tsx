import { useState, ReactNode } from 'react'

import { Settings } from '../../store/settings/types'
import Modal from '../../components/Modal'
import Button from '../../components/Button'
import Text from '../../components/Text'
import t from '../../locales'

import {
  MainContainer,
  MainContentContainer,
  Sidebar,
  MenuItem,
  MainContent,
  Footer,
} from './styles'
import WalletSettings from './WalletSettings'
import { SettingsProps, SettingsComponentProps } from './types'

const renderSettings = (
  settings: Settings,
  props: SettingsProps,
): ReactNode => {
  if (settings === Settings.Wallet) {
    return <WalletSettings {...props} />
  }

  return null
}

const SettingsComponent = ({
  open,
  onClose,
  activeSettings,
  goToSettings,
}: SettingsComponentProps) => {
  const [settingsChanged, setSettingsChanged] = useState(false)

  return (
    <Modal open={open} onClose={onClose}>
      <MainContainer>
        <MainContentContainer>
          <Sidebar>
            {Object.values(Settings)
              .filter(settingPage => t.common.nouns[settingPage])
              .map(settingPage => (
                <MenuItem
                  key={settingPage}
                  active={settingPage === activeSettings}
                  onClick={() => goToSettings(settingPage)}
                >
                  <Text
                    type={
                      settingPage === activeSettings
                        ? 'defaultHeavy'
                        : undefined
                    }
                  >
                    {t.common.nouns[settingPage]}
                  </Text>
                </MenuItem>
              ))}
          </Sidebar>
          <MainContent>
            {renderSettings(activeSettings, {
              onSettingsTouched: setSettingsChanged,
            })}
          </MainContent>
        </MainContentContainer>
        <Footer>
          <Button variant='secondary' onClick={onClose}>
            Cancel
          </Button>
          <Button disabled={!settingsChanged}>Save changes</Button>
        </Footer>
      </MainContainer>
    </Modal>
  )
}

SettingsComponent.defaultProps = {
  open: false,
  onClose: () => null,
}

export default SettingsComponent
