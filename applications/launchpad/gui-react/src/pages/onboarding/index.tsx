import { useState, useEffect } from 'react'
import TBotPrompt from '../../components/TBot/TBotPrompt'
import { OnboardingMessagesMap } from '../../config/onboardingMessagesConfig'

/**
 * @example Example of trigger to push new messages
    <div key='key1'>
      <p>Push new message</p>
      <button onClick={() => fireTrigger(true)}>Push</button>
    </div>
 */

const Onboarding = ({ close }: { close: () => void }) => {
  const [trigger, fireTrigger] = useState(false)

  const [messages, setMessages] = useState(OnboardingMessagesMap)

  // If we use currentIndex = 1, it will not show loading dots before rendering first message
  const [current, setCurrent] = useState(1)

  useEffect(() => {
    if (trigger) {
      const newMsgs = messages.slice()
      newMsgs.push({ content: 'Newly pushed', barFill: 0 })
      setMessages(newMsgs)
      setCurrent(newMsgs.length - 1)
      const hideLoadingDots = () => {
        setCurrent(newMsgs.length)
      }
      hideLoadingDots()
      fireTrigger(false)
    }
  }, [trigger])

  const addSkipMessages = () => {
    const newMsgs = messages.slice()
    newMsgs.push({
      content: (
        <div>
          <button onClick={() => setCurrent(newMsgs.length + 5)}>Skip</button>
        </div>
      ),
    })
    setMessages(newMsgs)
  }

  return (
    <div style={{ backgroundColor: '#FAFAFA' }}>
      <div>
        <p>Onboarding</p>
        <button onClick={close}>Go to home</button>
        <button onClick={addSkipMessages}>Add skip messages</button>
        <button
          onClick={() => {
            setMessages([])
            setCurrent(0)
          }}
        >
          Clear messages
        </button>
      </div>
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          width: '100vw',
          height: '100%',
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <TBotPrompt
          open={true}
          messages={messages}
          currentIndex={current}
          closeIcon={false}
          mode='onboarding'
          floating={false}
        />
      </div>
    </div>
  )
}

export default Onboarding
