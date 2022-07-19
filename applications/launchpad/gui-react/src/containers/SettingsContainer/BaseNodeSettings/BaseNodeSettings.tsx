import { useCallback } from 'react'
import { Controller, Control, UseFormSetValue } from 'react-hook-form'
import { open } from '@tauri-apps/api/dialog'
import { appDir } from '@tauri-apps/api/path'

import Select from '../../../components/Select'
import Text from '../../../components/Text'
import SettingsSectionHeader from '../../../components/SettingsSectionHeader'

import t from '../../../locales'
import { Network } from '../../BaseNodeContainer/types'
import { networkOptions } from '../../BaseNodeContainer/constants'

import { SettingsInputs } from '../types'
import { useTheme } from 'styled-components'
import { Label } from '../../../components/Inputs/Input/styles'
import { ConnectionRow, InputRow, SelectRow, TextWrapper } from './styles'
import Input from '../../../components/Inputs/Input'
import Button from '../../../components/Button'
import SvgInfo1 from '../../../styles/Icons/Info1'
import { useAppDispatch } from '../../../store/hooks'
import { tbotactions } from '../../../store/tbot'
import MessagesConfig from '../../../config/helpMessagesConfig'

const BaseNodeSettings = ({
  control,
  onBaseNodeConnectClick,
  setValue,
}: {
  control: Control<SettingsInputs>
  onBaseNodeConnectClick: () => void
  setValue: UseFormSetValue<SettingsInputs>
}) => {
  const theme = useTheme()
  const dispatch = useAppDispatch()
  const selectDirectory = useCallback(async (lastPath?: string) => {
    const selectedFolder = await open({
      directory: true,
      defaultPath: lastPath || (await appDir()),
    })

    if (selectedFolder === null) {
      return
    } else if (typeof selectedFolder === 'string') {
      setValue('baseNode.rootFolder', selectedFolder, {
        shouldDirty: true,
      })
    }
  }, [])

  return (
    <>
      <Text type='subheader' as='h2' color={theme.primary}>
        {t.baseNode.settings.title}
      </Text>
      <Controller
        name='baseNode.network'
        control={control}
        rules={{ required: true, minLength: 1 }}
        render={({ field }) => (
          <SelectRow>
            <Label $noMargin>{t.baseNode.tari_network_label}</Label>
            <div style={{ width: '50%' }}>
              <Select
                value={networkOptions.find(
                  ({ value }) => value === field.value,
                )}
                options={networkOptions}
                onChange={({ value }) => field.onChange(value as Network)}
                fullWidth
                styles={{ value: { color: theme.nodeWarningText } }}
              />
            </div>
          </SelectRow>
        )}
      />
      <SettingsSectionHeader noBottomMargin noTopMargin>
        {t.common.nouns.expert}
      </SettingsSectionHeader>
      <Controller
        name='baseNode.rootFolder'
        control={control}
        rules={{ required: true, minLength: 1 }}
        render={({ field }) => (
          <InputRow>
            <Label $noMargin>{t.baseNode.settings.rootFolder}</Label>
            <Input
              onClick={() => selectDirectory(field.value)}
              onChange={field.onChange}
              value={field?.value?.toString() || ''}
              containerStyle={{ width: '50%' }}
              withError={false}
              style={{ color: theme.nodeWarningText }}
            />
          </InputRow>
        )}
      />
      <ConnectionRow>
        <TextWrapper>
          <Text type='smallMedium' color={theme.helpTipText}>
            <Button
              variant='button-in-text'
              style={{ color: theme.onTextLight, fontSize: '14px' }}
              onClick={onBaseNodeConnectClick}
            >
              <Text type='smallMedium'>{t.common.verbs.connect}</Text>
            </Button>{' '}
            {t.baseNode.settings.aurora}
          </Text>
        </TextWrapper>
        <Button
          variant='button-in-text'
          onClick={() =>
            dispatch(tbotactions.push(MessagesConfig.ConnectAurora))
          }
        >
          <SvgInfo1 fontSize={22} color={theme.helpTipText} />
        </Button>
      </ConnectionRow>
    </>
  )
}

export default BaseNodeSettings
