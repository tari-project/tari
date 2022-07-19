import { ReactNode } from 'react'

import { Settings } from '../../store/settings/types'
import Modal from '../../components/Modal'
import Button from '../../components/Button'
import Switch from '../../components/Switch'
import Text from '../../components/Text'

import SvgSun from '../../styles/Icons/Sun'
import SvgMoon from '../../styles/Icons/Moon'

import t from '../../locales'

import {
  MainContainer,
  MainContentContainer,
  Sidebar,
  MenuItem,
  MainContent,
  Footer,
  DiscardWarning,
  SidebarTabs,
  SidebarSelectTheme,
  SwitchBg,
} from './styles'
import BaseNodeSettings from './BaseNodeSettings'
import MiningSettings from './MiningSettings'
import DockerSettings from './DockerSettings'
import WalletSettings from './WalletSettings'
import {
  SettingsProps,
  SettingsComponentProps,
  AuthenticationInputs,
} from './types'
import MoneroAuthentication from './MiningSettings/MoneroAuthentication'
import { useTheme } from 'styled-components'
import BaseNodeQRModal from '../BaseNodeQRModal'
import SecuritySettings from './SecuritySettings'

const renderSettings = (
  settings: Settings,
  props: SettingsProps,
): ReactNode => {
  switch (settings) {
    case Settings.Wallet:
      return <WalletSettings />
    case Settings.Mining:
      return (
        <MiningSettings
          formState={props.formState}
          control={props.control}
          values={props.values}
          setValue={props.setValue}
          setOpenMiningAuthForm={props.setOpenMiningAuthForm}
        />
      )
    case Settings.BaseNode:
      return (
        <BaseNodeSettings
          control={props.control}
          onBaseNodeConnectClick={props.onBaseNodeConnectClick}
          setValue={props.setValue}
        />
      )
    case Settings.Docker:
      return (
        <DockerSettings
          formState={props.formState}
          control={props.control}
          values={props.values}
          setValue={props.setValue}
          setOpenMiningAuthForm={props.setOpenMiningAuthForm}
        />
      )
    case Settings.Security:
      return <SecuritySettings />
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
  values,
  setValue,
  control,
  defaultMiningMergedValues,
  onSubmit,
  confirmCancel,
  cancelDiscard,
  discardChanges,
  openMiningAuthForm,
  setOpenMiningAuthForm,
  openBaseNodeConnect,
  setOpenBaseNodeConnect,
  currentTheme,
  changeTheme,
}: SettingsComponentProps) => {
  const theme = useTheme()
  // Render Monero Authentication form if open:
  if (openMiningAuthForm) {
    return (
      <Modal size='small' open={open}>
        <MoneroAuthentication
          defaultValues={
            defaultMiningMergedValues?.useAuth as
              | AuthenticationInputs
              | undefined
          }
          onSubmit={val => {
            setValue('mining.merged.authentication', val, { shouldDirty: true })
            setValue(
              'mining.merged.useAuth',
              Boolean(val.username || val.password),
              { shouldDirty: true },
            )
          }}
          close={() => setOpenMiningAuthForm(false)}
        />
      </Modal>
    )
  }

  // Render Base Node QR code
  if (openBaseNodeConnect) {
    return (
      <BaseNodeQRModal open onClose={() => setOpenBaseNodeConnect(false)} />
    )
  }

  // Render main Settings modal:
  return (
    <Modal
      open={open}
      onClose={onClose}
      style={{ border: `1px solid ${theme.selectBorderColor}` }}
    >
      <MainContainer data-testid='settings-modal-container'>
        <MainContentContainer>
          <Sidebar>
            <SidebarTabs>
              {Object.values(Settings)
                .filter(settingPage => t.common.nouns[settingPage])
                .map(settingPage => (
                  <MenuItem
                    key={settingPage}
                    active={settingPage === activeSettings}
                    onClick={() => goToSettings(settingPage)}
                  >
                    <Text
                      as='span'
                      type={
                        settingPage === activeSettings
                          ? 'smallHeavy'
                          : 'smallMedium'
                      }
                      style={{ paddingTop: 2 }}
                    >
                      {t.common.nouns[settingPage]}
                    </Text>
                  </MenuItem>
                ))}
            </SidebarTabs>
            <SidebarSelectTheme>
              <Text type='microMedium' color={theme.secondary}>
                {t.settings.selectTheme}
              </Text>
              <SwitchBg $transparent={currentTheme === 'dark'}>
                <Switch
                  leftLabel={<SvgSun width='1.4em' height='1.4em' />}
                  rightLabel={<SvgMoon width='1.4em' height='1.4em' />}
                  value={currentTheme === 'dark'}
                  onClick={v => changeTheme(v ? 'dark' : 'light')}
                />
              </SwitchBg>
            </SidebarSelectTheme>
          </Sidebar>
          <MainContent>
            {renderSettings(activeSettings, {
              formState,
              control,
              values,
              setValue,
              setOpenMiningAuthForm,
              onBaseNodeConnectClick: () => setOpenBaseNodeConnect(true),
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
