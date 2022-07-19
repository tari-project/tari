import t from '../../../../locales'
import Text from '../../../Text'
import GotItButton from '../GotItButton'
import { StyledTextContainer } from '../styles'

export const HowWalletWorks = (
  <>
    <StyledTextContainer>
      <Text type='defaultHeavy'>
        {t.wallet.helpMessages.howItWorks.title}{' '}
        <Text>{t.wallet.helpMessages.howItWorks.message}</Text>
      </Text>
    </StyledTextContainer>
    <GotItButton />
  </>
)

export const WhyBalanceDiffers = (
  <>
    <Text type='defaultHeavy'>
      {t.wallet.helpMessages.whyBalanceDiffers.title}
    </Text>
    <Text>{t.wallet.helpMessages.whyBalanceDiffers.message}</Text>
  </>
)

export const NoteAboutVerificationPeriod = () => {
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span'>
          {t.wallet.helpMessages.noteAboutVerificationPeriod.message}
        </Text>
      </StyledTextContainer>
      <GotItButton />
    </>
  )
}

export const TariWalletIdHelp = (
  <>
    <StyledTextContainer>
      <Text type='defaultHeavy'>
        {t.wallet.helpMessages.walletIdHelp.bold}{' '}
        <Text as='span'>{t.wallet.helpMessages.walletIdHelp.regular}</Text>
      </Text>
    </StyledTextContainer>
    <GotItButton />
  </>
)

export const TransactionFee = () => {
  return (
    <>
      <StyledTextContainer>
        <Text type='defaultMedium' as='span'>
          {t.wallet.helpMessages.transactionFee.message}
        </Text>
      </StyledTextContainer>
      <GotItButton />
    </>
  )
}
