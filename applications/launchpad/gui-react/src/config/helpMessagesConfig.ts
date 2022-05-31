import { Message1 } from '../components/TBot/HelpComponents/CryptoMining'
import { Message1 as Merged1 } from '../components/TBot/HelpComponents/MergedMining'
import { Message2 as Merged2 } from '../components/TBot/HelpComponents/MergedMining'
// @TODO: messages 3 - 6 are for dev purposes only!!
import { Message3 as Merged3 } from '../components/TBot/HelpComponents/MergedMining'
import { Message4 as Merged4 } from '../components/TBot/HelpComponents/MergedMining'
import { Message5 as Merged5 } from '../components/TBot/HelpComponents/MergedMining'
import { Message6 as Merged6 } from '../components/TBot/HelpComponents/MergedMining'
import { TBotMessages } from '../store/tbot/types'

const MessagesConfig = {
  [TBotMessages.CryptoMiningHelp]: ['cryptoHelpMessage1'],
  [TBotMessages.MergedMiningHelp]: [
    'mergedHelpMessage1',
    'mergedHelpMessage2',
    'mergedHelpMessage3',
    'mergedHelpMessage4',
    'mergedHelpMessage5',
    'mergedHelpMessage6',
  ],
}

export const HelpMessagesMap: { [key: string]: React.FC } = {
  cryptoHelpMessage1: Message1,
  mergedHelpMessage1: Merged1,
  mergedHelpMessage2: Merged2,
  mergedHelpMessage3: Merged3,
  mergedHelpMessage4: Merged4,
  mergedHelpMessage5: Merged5,
  mergedHelpMessage6: Merged6,
}

export default MessagesConfig
