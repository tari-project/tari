import { ReactNode } from 'react'
import { Message1 } from '../components/TBot/HelpComponents/CryptoMining'
import { Message1 as Merged1 } from '../components/TBot/HelpComponents/MergedMining'
import { Message2 as Merged2 } from '../components/TBot/HelpComponents/MergedMining'
import { TBotMessage } from '../components/TBot/TBotPrompt/types'
import { TBotMessages } from '../store/tbot/types'

const MessagesConfig = {
  [TBotMessages.CryptoMiningHelp]: ['cryptoHelpMessage1'],
  [TBotMessages.MergedMiningHelp]: ['mergedHelpMessage1', 'mergedHelpMessage2'],
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
}

export default MessagesConfig
