import { TBotMessages } from './../store/tbot/types'
import {
  Message1,
  Message2,
  Message3,
  Message4,
} from '../components/Onboarding/OnboardingMessages'
import { TBotMessage } from '../components/TBot/TBotPrompt/types'

const OnBoardingMessagesConfig = {
  [TBotMessages.Onboarding]: [
    'onboardingMessage1',
    'onboardingMessage2',
    'onboardingMessage3',
  ],
}

export const OnboardingMessagesMap: (string | TBotMessage)[] = [
  {
    content: Message1,
    wait: 500,
  },
  {
    content: Message2,
    wait: 5000,
  },
  {
    content: Message3,
    wait: 5000,
  },
  {
    content: Message4,
    wait: 5000,
  },
]

export default OnBoardingMessagesConfig
