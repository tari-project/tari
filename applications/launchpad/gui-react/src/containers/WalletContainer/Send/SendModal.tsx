import { useEffect, useRef, useState } from 'react'
import { useTheme } from 'styled-components'
import { useForm, Controller, SubmitHandler } from 'react-hook-form'
import { invoke } from '@tauri-apps/api'

import Button from '../../../components/Button'
import Input from '../../../components/Inputs/Input'
import Modal from '../../../components/Modal'
import Text from '../../../components/Text'
import Textarea from '../../../components/Inputs/Textarea'
import AmountInput from '../../../components/Inputs/AmountInput'
import Tag from '../../../components/Tag'

import t from '../../../locales'
import { formatAmount, toMicroT, toT } from '../../../utils/Format'

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
  ErrorContainer,
} from './styles'

import SvgTariSignetGradient from '../../../styles/Icons/TariSignetGradient'
import SvgTBotLoading from '../../../styles/Icons/TBotLoading'
import SvgTBotSearch from '../../../styles/Icons/TBotSearch'
import SvgCloseCross from '../../../styles/Icons/CloseCross'

import WalletConfig from '../../../config/wallet'
import useTransactionsRepository from '../../../persistence/transactionsRepository'
import {
  TransactionDirection,
  TransactionEvent,
} from '../../../useWalletEvents'
import { useAppSelector } from '../../../store/hooks'
import { selectWalletPublicKey } from '../../../store/wallet/selectors'

const defaultValues = {
  amount: 0,
  address: '',
  message: '',
}

interface SendTransferRecord {
  address: string
  failure_message?: string
  is_success: boolean
  transaction_id: number
}

interface SendTransferResponse {
  payments: SendTransferRecord[]
}

/**
 * Modal with the send transaction form.
 * @param {boolean} open - is modal open
 * @param {() => void} onClose - close the modal
 * @param {number} available - available
 */
const SendModal = ({ open, onClose, available }: SendModalProps) => {
  const theme = useTheme()
  const transactionsRepository = useTransactionsRepository()
  const intervalRef = useRef<ReturnType<typeof setInterval> | undefined>()

  const walletPublicKey = useAppSelector(selectWalletPublicKey)

  const [fee, setFee] = useState(
    WalletConfig.defaultFee * WalletConfig.defaultFeePerGram,
  )
  const [isProcessing, setIsProcessing] = useState(false)
  const [result, setResult] = useState<
    null | 'processing' | 'pending' | 'completing'
  >(null)
  const [error, setError] = useState<string | undefined>(undefined)
  const [counter, setCounter] = useState(0)
  const [tx, setTx] = useState<SendTransferRecord | undefined>(undefined)

  const { control, handleSubmit, reset, formState } = useForm<SendForm>({
    mode: 'onChange',
    defaultValues,
  })

  const cancel = () => {
    reset(defaultValues)
    setIsProcessing(false)
    setResult(null)
    setCounter(0)
    onClose()
  }

  const onSubmitForm: SubmitHandler<SendForm> = async (data: SendForm) => {
    setError(undefined)
    setIsProcessing(true)

    try {
      const sendResult: SendTransferResponse = await invoke('transfer', {
        funds: {
          payments: [
            {
              address: data.address,
              amount: toMicroT(data.amount),
              fee_per_gram: 1,
              message: data.message,
              payment_type: 0,
            },
          ],
        },
      })

      if (sendResult?.payments?.length > 0) {
        const sendTx = sendResult.payments[0]
        if (sendTx.is_success) {
          setTx(sendTx)
          setResult('processing')
          transactionsRepository.addOrReplace({
            event: TransactionEvent.Initialized,
            tx_id: sendTx.transaction_id.toString(),
            source_pk: walletPublicKey,
            dest_pk: sendTx.address,
            status: 'initialized',
            direction: TransactionDirection.Outbound,
            amount: toMicroT(data.amount),
            message: data.message || '',
            is_coinbase: false,
          })
        } else {
          setError(sendTx.failure_message)
        }
      }
      setIsProcessing(false)
    } catch (err) {
      setError((err as unknown as Error).toString())
      setIsProcessing(false)
    }
  }

  const validateAmount = (amount: number) => {
    if (amount === 0) {
      return
    }

    if (amount + fee >= available) {
      return t.wallet.transaction.errors.exceedsAvailableAndFee
    }

    return
  }

  useEffect(() => {
    const getTxFee = async () => {
      try {
        const txFee: number = await invoke('transaction_fee')
        setFee(txFee * WalletConfig.defaultFeePerGram)
      } catch (err) {
        // eslint-disable-next-line no-console
        console.log('Cannot get latest transaction fee', err)
      }
    }

    if (open) {
      getTxFee()
    }
  }, [open])

  useEffect(() => {
    if (result === 'processing' && tx) {
      intervalRef.current = setInterval(async () => {
        if (counter > 5) {
          setResult('pending')
          setCounter(0)
          clearInterval(intervalRef.current)
        } else {
          const foundTx = await transactionsRepository.findById(
            tx.transaction_id.toString(),
          )
          if (foundTx && foundTx.event !== TransactionEvent.Initialized) {
            setResult('completing')
            clearInterval(intervalRef.current)
            setCounter(0)
          }
        }

        setCounter(c => c + 1)
      }, 500)
    }

    return () => {
      if (intervalRef?.current) {
        clearInterval(intervalRef.current)
      }
    }
  }, [result, counter])

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

  if (isProcessing || result === 'processing') {
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
                fee={toT(fee)}
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
          {error && (
            <ErrorContainer onClick={() => setError(undefined)}>
              <Text type='microMedium'>{error}</Text>
              <SvgCloseCross
                style={{
                  position: 'absolute',
                  top: 8,
                  right: 8,
                }}
              />
            </ErrorContainer>
          )}
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
