import { useTheme } from 'styled-components'
import { useForm, Controller, SubmitHandler } from 'react-hook-form'

import Button from '../../../components/Button'
import Input from '../../../components/Inputs/Input'
import Modal from '../../../components/Modal'
import Text from '../../../components/Text'

import t from '../../../locales'
import { formatAmount } from '../../../utils/Format'

import { SendForm, SendModalProps } from './types'
import {
  StyledSendForm,
  SendFormContent,
  FormButtons,
  TagWrapper,
  TagBox,
  PleaseWaitContainer,
  CtaButtonContainer,
  ResultModal,
  ResultModalContent,
  ResultHeader,
} from './styles'
import Textarea from '../../../components/Inputs/Textarea'
import AmountInput from '../../../components/Inputs/AmountInput'
import SvgTariSignetGradient from '../../../styles/Icons/TariSignetGradient'
import { useState } from 'react'
import SvgTBotLoading from '../../../styles/Icons/TBotLoading'
import Tag from '../../../components/Tag'
import SvgTBotSearch from '../../../styles/Icons/TBotSearch'

const defaultValues = {
  amount: 0,
  address: '',
  message: '',
}

/**
 * Modal with the send transaction form.
 * @param {boolean} open - is modal open
 * @param {() => void} onClose - close the modal
 * @param {number} available - available
 */
const SendModal = ({ open, onClose, available }: SendModalProps) => {
  const theme = useTheme()
  /**
   * @TODO - replace with real data
   */
  const fee = 0.00043

  /** ---- MOCK SEND PROCESS ------- */
  const [isProcessing, setIsProcessing] = useState(false)
  const [result, setResult] = useState<null | 'pending' | 'completing'>(null)
  /** ---- END MOCK SEND PROCESS ------- */

  const { control, handleSubmit, reset, formState } = useForm<SendForm>({
    mode: 'onChange',
    defaultValues,
  })

  const cancel = () => {
    reset(defaultValues)
    setIsProcessing(false)
    setResult(null)
    onClose()
  }

  const onSubmitForm: SubmitHandler<SendForm> = _ => {
    reset(defaultValues)
    setIsProcessing(true)
  }

  const validateAmount = (amount: number) => {
    if (amount > available + fee) {
      return t.wallet.transaction.errors.exceedsAvailableAndFee
    }

    if (amount === 0) {
      return
    }

    return
  }

  if (result === 'pending') {
    return (
      <Modal
        open={open}
        onClose={cancel}
        size='small'
        style={{ border: `1px solid ${theme.selectBorderColor}` }}
      >
        <ResultModal>
          <ResultModalContent>
            <ResultHeader>
              <Text type='subheader' color={theme.primary}>
                {t.common.phrases.yourJobIsDoneHere}!
              </Text>
              <Tag>{t.wallet.transaction.transactionPending}</Tag>
              <Text
                type='smallMedium'
                style={{ textAlign: 'center' }}
                color={theme.primary}
              >
                {t.wallet.transaction.transactionPendingDesc1}
              </Text>
            </ResultHeader>
            <SvgTBotSearch
              width={100}
              height={100}
              style={{ marginBottom: theme.spacingVertical(1.5) }}
            />
            <Text
              type='microMedium'
              style={{ textAlign: 'center' }}
              color={theme.nodeWarningText}
            >
              {t.wallet.transaction.transactionPendingDesc2}
            </Text>
          </ResultModalContent>
          <CtaButtonContainer>
            <Button onClick={cancel} fullWidth>
              {`${t.common.phrases.gotIt}!`}
            </Button>
          </CtaButtonContainer>
        </ResultModal>
      </Modal>
    )
  }

  if (result === 'completing') {
    return (
      <Modal
        open={open}
        onClose={cancel}
        size='small'
        style={{ border: `1px solid ${theme.selectBorderColor}` }}
      >
        <ResultModal>
          <ResultModalContent>
            <ResultHeader>
              <Text type='subheader' color={theme.primary}>
                {t.common.phrases.yourJobIsDoneHere}!
              </Text>
              <Tag>{t.wallet.transaction.completingFinalProcessing}</Tag>
            </ResultHeader>
            <SvgTBotLoading
              width={100}
              height={100}
              style={{ marginBottom: theme.spacingVertical(1.5) }}
            />
            <Text
              type='smallMedium'
              style={{ textAlign: 'center' }}
              color={theme.primary}
            >
              {t.wallet.transaction.completingDescription}
            </Text>
          </ResultModalContent>
          <CtaButtonContainer>
            <Button onClick={cancel} fullWidth>
              {`${t.common.phrases.gotIt}!`}
            </Button>
          </CtaButtonContainer>
        </ResultModal>
      </Modal>
    )
  }

  if (isProcessing) {
    return (
      <Modal
        open={open}
        size='small'
        style={{ border: `1px solid ${theme.selectBorderColor}` }}
      >
        <PleaseWaitContainer>
          <SvgTBotLoading
            width={100}
            height={100}
            style={{ marginBottom: theme.spacingVertical(1.5) }}
          />
          <Text type='subheader' color={theme.primary}>
            {t.common.phrases.pleaseWait}
          </Text>
          <Text type='smallMedium' color={theme.primary}>
            {t.wallet.transaction.searchingForRecipient}
          </Text>
          {/* @TODO: remove these buttons when transactions are finalised */}
          <button onClick={() => setResult('pending')}>Result 1</button>
          <button onClick={() => setResult('completing')}>Result 2</button>
        </PleaseWaitContainer>
      </Modal>
    )
  }

  return (
    <Modal
      open={open}
      onClose={cancel}
      size='small'
      style={{ border: `1px solid ${theme.selectBorderColor}` }}
    >
      <StyledSendForm onSubmit={handleSubmit(onSubmitForm)}>
        <SendFormContent>
          <TagWrapper>
            <TagBox>
              <Text type='smallMedium' color={theme.nodeWarningText}>
                {t.wallet.balance.available}{' '}
                <Text as='span' type='smallHeavy' color={theme.primary}>
                  {formatAmount(available)}
                </Text>{' '}
                XTR
              </Text>
            </TagBox>
          </TagWrapper>

          <Controller
            name='amount'
            control={control}
            rules={{
              validate: { validateAmount },
            }}
            render={({ field }) => (
              <AmountInput
                testId='send-amount-input'
                maxDecimals={2}
                icon={<SvgTariSignetGradient />}
                onChange={field.onChange}
                value={field.value}
                currency='XTR'
                autoFocus
                withFee
                fee={fee}
                withError
                error={formState.errors.amount?.message}
              />
            )}
          />

          <Controller
            name='address'
            control={control}
            rules={{
              required: true,
              minLength: {
                value: 12,
                message: t.wallet.transaction.errors.recipientIdError,
              },
            }}
            render={({ field }) => (
              <Input
                label={t.wallet.transaction.form.recipientIdAddress}
                placeholder={t.wallet.transaction.form.recipientIdPlacehoder}
                testId='send-address-input'
                error={formState.errors.address?.message}
                {...field}
              />
            )}
          />

          <Controller
            name='message'
            control={control}
            rules={{
              maxLength: {
                value: 250,
                message: t.wallet.transaction.errors.messageIsTooLong,
              },
            }}
            render={({ field }) => (
              <Textarea
                placeholder={t.wallet.transaction.form.messagePlaceholder}
                label={t.wallet.transaction.form.messageOptional}
                testId='send-message-input'
                rows={5}
                style={{ resize: 'none' }}
                withError
                error={formState.errors.message?.message}
                {...field}
              />
            )}
          />
        </SendFormContent>

        <FormButtons>
          <Button variant='secondary' onClick={cancel}>
            {t.common.verbs.cancel}
          </Button>
          <Button
            variant='primary'
            type='submit'
            fullWidth
            disabled={!formState.isValid || formState.isSubmitting}
          >
            {t.wallet.transaction.form.sendFunds}
          </Button>
        </FormButtons>
      </StyledSendForm>
    </Modal>
  )
}

export default SendModal
