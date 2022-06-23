import { ReactNode } from 'react'

import { Message1 } from '../components/TBot/HelpComponents/CryptoMining'
import {
  Message1 as Merged1,
  Message2 as Merged2,
} from '../components/TBot/HelpComponents/MergedMining'
import {
  HowWalletWorks,
  WhyBalanceDiffers,
  NoteAboutVerificationPeriod,
} from '../components/TBot/HelpComponents/Wallet'
import { WhatIsBaseNode } from '../components/TBot/HelpComponents/BaseNode'
import { TBotMessage } from '../components/TBot/TBotPrompt/types'
import { TBotMessages } from '../store/tbot/types'

const MessagesConfig = {
  [TBotMessages.CryptoMiningHelp]: ['cryptoHelpMessage1'],
  [TBotMessages.MergedMiningHelp]: ['mergedHelpMessage1', 'mergedHelpMessage2'],
  [TBotMessages.WalletHelp]: ['walletHelpMessage'],
  [TBotMessages.WalletBalanceHelp]: [
    'whyBalanceDiffers',
    'noteAboutVerificationPeriod',
  ],
  [TBotMessages.BaseNodeHelp]: ['whatIsBaseNode'],
}

export const HelpMessagesMap: {
  [key: string]: string | ReactNode | TBotMessage
} = {
  cryptoHelpMessage1: {
    content: Message1,
  },
  mergedHelpMessage1: Merged1,
  mergedHelpMessage2: {
    content: Merged2,
  },
  walletHelpMessage: HowWalletWorks,
  whyBalanceDiffers: WhyBalanceDiffers,
  noteAboutVerificationPeriod: {
    content: NoteAboutVerificationPeriod,
  },
  whatIsBaseNode: {
    content: WhatIsBaseNode,
  },
}

export default MessagesConfig
