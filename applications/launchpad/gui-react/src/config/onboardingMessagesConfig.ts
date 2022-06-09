import { TBotMessages } from './../store/tbot/types'
import { Message1, Message2 } from '../components/Onboarding/OnboardingMessages'
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
    barFill: 0.05,
  },
  {
    content: Message2,
  },
]

export default OnBoardingMessagesConfig
