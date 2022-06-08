import { useEffect, useState } from 'react'
import { useForm, SubmitHandler } from 'react-hook-form'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { selectMergedMiningState } from '../../store/mining/selectors'
import { actions } from '../../store/settings'
import {
  selectSettingsOpen,
  selectActiveSettings,
} from '../../store/settings/selectors'
import { saveSettings } from '../../store/settings/thunks'

import SettingsComponent from './SettingsComponent'
import { SettingsInputs } from './types'

const SettingsContainer = () => {
  const dispatch = useAppDispatch()
  const settingsOpen = useAppSelector(selectSettingsOpen)
  const activeSettings = useAppSelector(selectActiveSettings)

  const miningMerged = useAppSelector(selectMergedMiningState)

  const [confirmCancel, setConfirmCancel] = useState(false)

  const { control, handleSubmit, formState, reset } = useForm<SettingsInputs>({
    mode: 'onChange',
    defaultValues: {
      mining: {
        merged: {
          address: miningMerged.address,
          threads: miningMerged.threads,
          urls: miningMerged.urls,
        },
      },
    },
  })

  useEffect(() => {
    if (settingsOpen === true) {
      reset({
        mining: {
          merged: {
            address: miningMerged.address,
            threads: miningMerged.threads,
            urls: miningMerged.urls,
          },
        },
      })
    }
  }, [settingsOpen])

  const onSubmit: SubmitHandler<SettingsInputs> = async data => {
    await dispatch(saveSettings({ newSettings: data }))
    reset(data)
    dispatch(actions.close())
  }

  const tryToClose = () => {
    if (formState.isSubmitting) {
      return
    }

    if (!formState.isDirty) {
      dispatch(actions.close())
      return
    }

    setConfirmCancel(true)
  }

  const closeAndDiscard = () => {
    setConfirmCancel(false)
    if (formState.isDirty) {
      reset()
    }
    dispatch(actions.close())
  }

  return (
    <SettingsComponent
      open={settingsOpen}
      onClose={tryToClose}
      activeSettings={activeSettings}
      goToSettings={settingsPage => dispatch(actions.goTo(settingsPage))}
      formState={formState}
      onSubmit={() => handleSubmit(onSubmit)()}
      control={control}
      confirmCancel={confirmCancel}
      cancelDiscard={() => setConfirmCancel(false)}
      discardChanges={closeAndDiscard}
    />
  )
}

export default SettingsContainer
