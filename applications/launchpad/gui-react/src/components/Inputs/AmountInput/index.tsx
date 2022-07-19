import { ChangeEvent, useState } from 'react'
import { useDispatch } from 'react-redux'

import SvgQuestion from '../../../styles/Icons/Question'
import Button from '../../Button'
import Text from '../../Text'

import t from '../../../locales'
import MessagesConfig from '../../../config/helpMessagesConfig'
import { tbotactions } from '../../../store/tbot'

import {
  IconWrapper,
  InputContainer,
  InputWrapper,
  StyledAmountInput,
  StyledInput,
  Currency,
  TransactionFee,
  ErrorContainer,
} from './styles'
import { AmountInputProps } from './types'
import { useTheme } from 'styled-components'

const whatIsDecimalSeparator = () => {
  const n = 1.1
  return n.toLocaleString().substring(1, 2)
}

const ds = whatIsDecimalSeparator()

const AmountInput = ({
  value = 0,
  disabled,
  onChange,
  icon,
  error,
  withError,
  maxDecimals,
  currency,
  autoFocus,
  withFee,
  fee,
  testId,
}: AmountInputProps) => {
  const dispatch = useDispatch()
  const theme = useTheme()

  const [valStr, setValStr] = useState(value.toString())

  const onChangeLocal = (e: ChangeEvent<HTMLInputElement>) => {
    // Remove all non-digits and delimiter characters:
    let newVal = e.target.value.replace(new RegExp(`[^0-9${ds}]`, 'g'), '')
    // Remove all decimal delimiters expect the last one:
    newVal = newVal.replace(new RegExp(`[${ds}](?=${ds}*[${ds}])`, 'g'), '')
    // Remove leading zeros:
    newVal = newVal.replace(/^0+(\d)/, '$1')

    // Limit number of decimals (optionally)
    if (maxDecimals && newVal.includes(ds)) {
      const splitted = newVal.split('.')
      if (splitted.length > 1) {
        newVal = splitted[0] + '.' + splitted[1].substring(0, maxDecimals)
      }
    }

    newVal = newVal === '' ? '0' : newVal
    setValStr(newVal)
    onChange(Number(newVal))
  }

  return (
    <StyledAmountInput>
      <InputContainer>
        {icon && <IconWrapper>{icon}</IconWrapper>}
        <InputWrapper>
          <StyledInput
            onChange={onChangeLocal}
            value={valStr}
            autoFocus={autoFocus}
            data-testid={testId || 'amount-input-cmp'}
            disabled={disabled}
            style={{ background: 'transparent' }}
          />
        </InputWrapper>
        {currency && (
          <Currency>
            <Text as='span' color={theme.helpTipText}>
              {currency}
            </Text>
          </Currency>
        )}
      </InputContainer>

      {withFee && (
        <TransactionFee>
          {value > 0 ? (
            <>
              <Text as='span' type='microMedium' color={theme.helpTipText}>
                {t.wallet.transaction.transactionFee}
              </Text>
              <Text
                as='span'
                type='microMedium'
                color={theme.primary}
                data-testId='fee-text'
              >
                +{fee}
              </Text>
              <Button
                variant='button-in-text'
                leftIcon={<SvgQuestion />}
                onClick={() =>
                  dispatch(tbotactions.push(MessagesConfig.TransactionFee))
                }
                style={{ marginTop: -2, color: theme.primary }}
                testId='fee-help-button'
              />
            </>
          ) : null}
        </TransactionFee>
      )}

      {withError && (
        <ErrorContainer>
          {Boolean(error) && (
            <Text
              type='microMedium'
              style={{
                marginTop: theme.spacingVertical(0.2),
                fontStyle: 'italic',
                color: theme.warningDark,
              }}
            >
              {error}
            </Text>
          )}
        </ErrorContainer>
      )}
    </StyledAmountInput>
  )
}

export default AmountInput
