import { useMemo, useEffect, useState } from 'react'
import { useForm, SubmitHandler } from 'react-hook-form'
import { setTheme } from '../../store/app'
import { selectTheme } from '../../store/app/selectors'

import { useAppSelector, useAppDispatch } from '../../store/hooks'
import { selectMergedMiningState } from '../../store/mining/selectors'
import { actions } from '../../store/settings'
import {
  selectSettingsOpen,
  selectActiveSettings,
  selectServiceSettings,
} from '../../store/settings/selectors'
import { selectNetwork, selectRootFolder } from '../../store/baseNode/selectors'
import { saveSettings } from '../../store/settings/thunks'
import { ThemeType } from '../../styles/themes/types'

import SettingsComponent from './SettingsComponent'
import { SettingsInputs } from './types'

const SettingsContainer = () => {
  const dispatch = useAppDispatch()
  const settingsOpen = useAppSelector(selectSettingsOpen)
  const activeSettings = useAppSelector(selectActiveSettings)

  const miningMerged = useAppSelector(selectMergedMiningState)
  const serviceSettings = useAppSelector(selectServiceSettings)
  const baseNodeNetwork = useAppSelector(selectNetwork)
  const baseNodeRootFolder = useAppSelector(selectRootFolder)
  const currentTheme = useAppSelector(selectTheme)

  const [openMiningAuthForm, setOpenMiningAuthForm] = useState(false)
  const [openBaseNodeConnect, setOpenBaseNodeConnect] = useState(false)
  const [confirmCancel, setConfirmCancel] = useState(false)

  const defaultValues = useMemo(
    () => ({
      mining: {
        merged: {
          address: miningMerged.address,
          threads: miningMerged.threads,
          urls: miningMerged.urls,
          useAuth: miningMerged.useAuth,
        },
      },
      docker: {
        tag: serviceSettings.dockerTag,
        registry: serviceSettings.dockerRegistry,
      },
      baseNode: {
        rootFolder: baseNodeRootFolder,
        network: baseNodeNetwork,
      },
    }),
    [miningMerged, serviceSettings],
  )

  const { control, handleSubmit, formState, reset, setValue, getValues } =
    useForm<SettingsInputs>({
      mode: 'onChange',
      defaultValues: defaultValues,
    })

  useEffect(() => {
    if (settingsOpen === true) {
      reset(defaultValues)
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

  const changeTheme = (theme: ThemeType) => {
    dispatch(setTheme(theme))
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
      values={getValues()}
      setValue={setValue}
      onSubmit={() => handleSubmit(onSubmit)()}
      control={control}
      defaultMiningMergedValues={getValues().mining.merged}
      confirmCancel={confirmCancel}
      cancelDiscard={() => setConfirmCancel(false)}
      discardChanges={closeAndDiscard}
      openMiningAuthForm={openMiningAuthForm}
      setOpenMiningAuthForm={setOpenMiningAuthForm}
      openBaseNodeConnect={openBaseNodeConnect}
      setOpenBaseNodeConnect={setOpenBaseNodeConnect}
      currentTheme={currentTheme}
      changeTheme={changeTheme}
    />
  )
}

export default SettingsContainer
