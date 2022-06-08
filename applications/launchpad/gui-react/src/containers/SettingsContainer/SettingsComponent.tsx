import { ReactNode } from 'react'

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
  DiscardWarning,
} from './styles'
import BaseNodeSettings from './BaseNodeSettings'
import MiningSettings from './MiningSettings'
import WalletSettings from './WalletSettings'
import { SettingsProps, SettingsComponentProps } from './types'

const renderSettings = (
  settings: Settings,
  props: SettingsProps,
): ReactNode => {
  switch (settings) {
    case Settings.Wallet:
      return <WalletSettings />
    case Settings.Mining:
      return <MiningSettings control={props.control} />
    case Settings.BaseNode:
      return <BaseNodeSettings control={props.control} />
    default:
      return null
  }
}

const SettingsComponent = ({
  open,
  onClose,
  activeSettings,
  goToSettings,
  formState,
  control,
  onSubmit,
  confirmCancel,
  cancelDiscard,
  discardChanges,
}: SettingsComponentProps) => {
  return (
    <Modal open={open} onClose={onClose}>
      <MainContainer data-testid='settings-modal-container'>
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
              control,
            })}
          </MainContent>
        </MainContentContainer>
        {confirmCancel && (
          <Footer>
            <DiscardWarning>
              <Text type='smallHeavy'>{t.settings.discardChanges}</Text>
              <Text type='smallMedium'>{t.settings.discardChangesDesc}.</Text>
            </DiscardWarning>
            <Button variant='secondary' onClick={cancelDiscard} size='small'>
              {t.common.phrases.keepEditing}
            </Button>
            <Button
              disabled={!formState.isDirty || formState.isSubmitting}
              onClick={discardChanges}
              loading={formState.isSubmitting}
              variant='warning'
              size='small'
            >
              {t.settings.closeAndDiscard}
            </Button>
          </Footer>
        )}
        {!confirmCancel && (
          <Footer>
            <Button variant='secondary' onClick={onClose}>
              {t.common.verbs.cancel}
            </Button>
            <Button
              type='submit'
              disabled={
                !formState.isDirty ||
                formState.isSubmitting ||
                !formState.isValid
              }
              onClick={onSubmit}
              loading={formState.isSubmitting}
              testId='settings-submit-btn'
            >
              {t.common.phrases.saveChanges}
            </Button>
          </Footer>
        )}
      </MainContainer>
    </Modal>
  )
}

SettingsComponent.defaultProps = {
  open: false,
  onClose: () => null,
}

export default SettingsComponent
