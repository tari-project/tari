import { TBotMessages } from './../store/tbot/types'
import messages from '../components/Onboarding/OnboardingMessages'
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
    content: messages[0],
    wait: 500,
  },
  {
    content: messages[1],
    wait: 5000,
  },
  {
    content: messages[2],
    wait: 5000,
  },
  {
    content: messages[3],
    wait: 5000,
  },
  {
    content: messages[4],
    wait: 5000,
    noSkip: true,
  },
]

export default OnBoardingMessagesConfig
