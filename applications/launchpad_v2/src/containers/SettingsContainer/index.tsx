import { useState, ReactNode, ChangeEvent } from 'react'
import styled from 'styled-components'

import Modal from '../../components/Modal'
import Button from '../../components/Button'
import Text from '../../components/Text'
import { Settings } from '../../store/settings/types'
import t from '../../locales'

const MainContainer = styled.div`
  display: flex;
  flex-direction: column;
  height: 100%;
`

const MainContentContainer = styled.div`
  display: flex;
  flex-grow: 1;
`

const Sidebar = styled.aside`
  width: 160px;
  min-width: 160px;
  height: 100%;
  border-right: 1px solid ${({ theme }) => theme.borderColor};
  padding-top: ${({ theme }) => theme.spacing()};
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  row-gap: ${({ theme }) => theme.spacing(0.75)};
  align-items: flex-end;
`

const MenuItem = styled.button<{ active?: boolean }>`
  margin: 0;
  padding: 0;
  outline: none;
  border: none;
  text-align: left;
  cursor: pointer;
  border-top-left-radius: ${({ theme }) => theme.tightBorderRadius(0.75)};
  border-bottom-left-radius: ${({ theme }) => theme.tightBorderRadius(0.75)};
  background: ${({ theme, active }) =>
    active ? theme.backgroundImage : 'none'};
  box-sizing: border-box;
  padding: ${({ theme }) => theme.spacingVertical()} 0;
  padding-left: ${({ theme }) => theme.spacingHorizontal()};
  width: 136px;
  color: ${({ theme, active }) => (active ? theme.accent : theme.accentDark)};

  &:hover {
    background: ${({ theme }) => theme.backgroundImage};
    color: ${({ theme }) => theme.accent};
  }
`

const Footer = styled.footer`
  display: flex;
  justify-content: flex-end;
  align-items: center;
  padding: ${({ theme }) => theme.spacingVertical()}
    ${({ theme }) => theme.spacingHorizontal()};
  column-gap: ${({ theme }) => theme.spacing()};
  border-top: 1px solid ${({ theme }) => theme.borderColor};
`

const MainContent = styled.main`
  flex-grow: 1;
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  align-items: center;
  & > * {
    max-width: 100%;
    width: 60%;
  }
`

const WalletSettings = ({
  onClose,
  onSettingsChanged,
}: {
  onClose: () => void
  onSettingsChanged: (changed: boolean) => void
}) => {
  const onChange = (event: ChangeEvent<HTMLInputElement>) => {
    const { checked } = event.target

    onSettingsChanged(checked)
  }

  return (
    <>
      <p>wallet settings</p>
      <input type='checkbox' onChange={onChange} /> changed settings
      <button onClick={onClose}>close</button>
    </>
  )
}

const renderSettings = (
  settings: Settings,
  props: {
    onClose: () => void
    onSettingsChanged: (changed: boolean) => void
  },
): ReactNode => {
  if (settings === Settings.Wallet) {
    return <WalletSettings {...props} />
  }

  return null
}

const SettingsContainer = ({
  open,
  onClose,
}: {
  open?: boolean
  onClose: () => void
}) => {
  const [activeSettings, openSettings] = useState(Settings.Wallet)
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
                  onClick={() => openSettings(settingPage)}
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
              onClose,
              onSettingsChanged: setSettingsChanged,
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

SettingsContainer.defaultProps = {
  open: false,
  onClose: () => null,
}

export default SettingsContainer
