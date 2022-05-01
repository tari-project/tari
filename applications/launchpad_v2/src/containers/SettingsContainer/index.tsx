import { useState, ReactNode, ChangeEvent } from 'react'
import styled, { useTheme } from 'styled-components'
import { clipboard } from '@tauri-apps/api'

import Modal from '../../components/Modal'
import Button from '../../components/Button'
import Text from '../../components/Text'
import Tag from '../../components/Tag'
import Box from '../../components/Box'
import Loading from '../../components/Loading'
import CopyIcon from '../../styles/Icons/Copy'
import { Settings } from '../../store/settings/types'
import t from '../../locales'

const address = '7a6ffed9-4252-427e-af7d-3dcaaf2db2df'

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
  padding-top: ${({ theme }) => theme.spacing(2)};
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
  padding-top: ${({ theme }) => theme.spacing(2)};
  display: flex;
  flex-direction: column;
  justify-content: flex-start;
  align-items: center;
  & > * {
    max-width: 100%;
    width: 70%;
  }
`

const Link = (props: any) => <a target='_blank' {...props} />

const CopyBox = ({ label, value }: { label: string; value: string }) => {
  const theme = useTheme()

  const copy = async () => {
    await clipboard.writeText(value)

    alert(`copied ${value}`)
  }

  return (
    <>
      <Text>{label}</Text>
      <span
        style={{
          background: theme.backgroundImage,
          border: `1px solid ${theme.borderColor}`,
          borderRadius: theme.tightBorderRadius(),
          color: theme.secondary,
          padding: `${theme.spacingVertical()} ${theme.spacingHorizontal()}`,
          margin: `${theme.spacingVertical()} 0`,
          boxSizing: 'border-box',
          display: 'flex',
          justifyContent: 'space-between',
        }}
      >
        {value}
        <Button variant='text' style={{ padding: 0, margin: 0 }} onClick={copy}>
          <CopyIcon />
        </Button>
      </span>
    </>
  )
}

const WalletSettings = ({
  onClose,
  onSettingsChanged,
}: {
  onClose: () => void
  onSettingsChanged: (changed: boolean) => void
}) => {
  const theme = useTheme()

  const onChange = (event: ChangeEvent<HTMLInputElement>) => {
    const { checked } = event.target

    onSettingsChanged(checked)
  }

  const running = true
  const pending = false

  return (
    <>
      <Text type='header'>Wallet Settings</Text>
      <Box
        style={{
          borderRadius: 0,
          borderLeft: 'none',
          borderRight: 'none',
          display: 'flex',
          justifyContent: 'space-between',
          paddingLeft: 0,
          paddingRight: 0,
        }}
      >
        <span
          style={{
            display: 'flex',
            alignItems: 'center',
            columnGap: theme.spacingVertical(),
          }}
        >
          <Text>Wallet</Text>
          {running && !pending ? (
            <Tag variant='small' type='running'>
              <span>{t.common.adjectives.running}</span>
            </Tag>
          ) : null}
          {pending ? <Loading loading={true} size='12px' /> : null}
        </span>
        <Button variant='secondary'>Stop</Button>
      </Box>
      <CopyBox label='Tari Wallet ID (address)' value={address} />
      <Text type='smallMedium' color={theme.secondary}>
        Mined Tari is stored in Launchpad&apos;s wallet. Send funds to wallet of
        your choice (try{' '}
        <Link href='https://aurora.tari.com/'>Tari Aurora</Link> - it&apos;s
        great!) and enjoy extended functionality (including payment requests,
        recurring payments, ecommerce payments and more). To do this, you may
        need to convert the ID to emoji format.
      </Text>
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
