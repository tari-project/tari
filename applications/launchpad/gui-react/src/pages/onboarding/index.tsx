import { useState, useEffect } from 'react'
import TBotPrompt from '../../components/TBot/TBotPrompt'

const Onboarding = ({ close }: { close: () => void }) => {
  const [trigger, fireTrigger] = useState(false)

  const [messages, setMessages] = useState([
    {
      content: 'Message number one',
      barFill: 0.063,
    },
    {
      content: 'Message number two',
      barFill: 0.125,
    },
    {
      content: 'Message number three',
      barFill: 0.188,
    },
    {
      content: 'Message number four',
      barFill: 0.25,
    },
    {
      content: 'need to wait longer...',
      wait: 6000,
      barFill: 0.3,
    },
    <div key='key1'>
      <p>Push new message</p>
      <button onClick={() => fireTrigger(true)}>Push</button>
    </div>,
  ])

  // If we use currentIndex = 1, it will not show loading dots before rendering first message
  const [current, setCurrent] = useState(0)

  // const [progressFill, setProgressFill] = useState(0)

  useEffect(() => {
    if (trigger) {
      const newMsgs = messages.slice()
      newMsgs.push({ content: 'Newly pushed', barFill: 0 })
      setMessages(newMsgs)
      setCurrent(newMsgs.length - 1)
      setCurrent(newMsgs.length) // it won't show loading dots when currentIndex is equal to the messages length
      fireTrigger(false)
    }
  }, [trigger])

  const addSkipMessages = () => {
    const newMsgs = messages.slice()
    newMsgs.push(
      <div>
        <button onClick={() => setCurrent(newMsgs.length + 5)}>Skip</button>
      </div>,
    )
    // newMsgs.push('msg 1/5')
    // newMsgs.push('msg 2/5')
    // newMsgs.push('msg 3/5')
    // newMsgs.push('msg 4/5')
    // newMsgs.push('msg 5/5')
    setMessages(newMsgs)
  }
  // console.log('CURRENT: ', current)
  return (
    <div>
      {/* <div
        style={{
          position: 'fixed',
          zIndex: 100,
          top: 0,
          left: 0,
          background: '#a99',
        }}
      >
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
      </div> */}
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
        />
      </div>
    </div>
  )
}

export default Onboarding
