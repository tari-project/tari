import { useForm, Controller, SubmitHandler } from 'react-hook-form'

import Button from '../../../components/Button'
import Text from '../../../components/Text'

import { useAppDispatch } from '../../../store/hooks'
import { MergedMiningSetupRequired } from '../../../store/mining/types'
import { actions as miningActions } from '../../../store/mining'

import t from '../../../locales'

import {
  FormTextWrapper,
  SetupMergedContent,
  SetupMergedFormContainer,
} from './styles'
import { useState } from 'react'
import Input from '../../../components/Inputs/Input'
import { useTheme } from 'styled-components'

const SetupMergedWithForm = ({
  mergedSetupRequired,
  changeTag,
}: {
  mergedSetupRequired: MergedMiningSetupRequired
  changeTag: () => void
}) => {
  const theme = useTheme()
  const dispatch = useAppDispatch()
  const [revealForm, setRevealForm] = useState(false)

  const { control, handleSubmit, formState } = useForm<{ address: string }>({
    mode: 'onChange',
  })

  const onSubmitForm: SubmitHandler<{ address: string }> = data => {
    dispatch(miningActions.setMergedAddress(data))
  }

  return (
    <SetupMergedContent>
      <Text style={{ maxWidth: 242 }}>
        {t.mining.setup.description}{' '}
        <Text as='span' type='defaultHeavy'>
          {t.mining.setup.descriptionBold}
        </Text>
      </Text>

      {!revealForm ? (
        <Button
          variant='primary'
          onClick={() => {
            setRevealForm(true)
            changeTag()
          }}
          disabled={
            mergedSetupRequired ===
            MergedMiningSetupRequired.MissingWalletAddress
          }
        >
          {t.mining.actions.setupAndStartMining}
        </Button>
      ) : null}

      {revealForm ? (
        <SetupMergedFormContainer>
          <form onSubmit={handleSubmit(onSubmitForm)}>
            <Controller
              name='address'
              control={control}
              defaultValue=''
              rules={{
                required: true,
                minLength: {
                  value: 12,
                  message: t.mining.settings.moneroAddressError,
                },
              }}
              render={({ field }) => (
                <Input
                  placeholder={t.mining.setup.addressPlaceholder}
                  testId='address-input'
                  autoFocus
                  error={formState.errors.address?.message}
                  {...field}
                />
              )}
            />
            <FormTextWrapper>
              <Text color={theme.primary} type='smallMedium'>
                {t.mining.setup.formDescription}
              </Text>
            </FormTextWrapper>
            <Button
              variant='primary'
              type='submit'
              disabled={!formState.isValid || formState.isSubmitting}
            >
              {t.mining.actions.setupAndStartMining}
            </Button>
          </form>
        </SetupMergedFormContainer>
      ) : null}
    </SetupMergedContent>
  )
}

export default SetupMergedWithForm
