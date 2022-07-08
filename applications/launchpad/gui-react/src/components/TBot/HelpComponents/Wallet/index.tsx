import t from '../../../../locales'
import Text from '../../../Text'
import GotItButton from '../GotItButton'
import { StyledTextContainer } from '../styles'

export const HowWalletWorks = (
  <>
    <Text type='defaultHeavy'>{t.wallet.helpMessages.howItWorks.title}</Text>
    <Text>{t.wallet.helpMessages.howItWorks.message}</Text>
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
    <Text type='defaultHeavy'>
      {t.wallet.helpMessages.walletIdHelp.bold}{' '}
      <Text as='span'>{t.wallet.helpMessages.walletIdHelp.regular}</Text>
    </Text>
  </>
)
