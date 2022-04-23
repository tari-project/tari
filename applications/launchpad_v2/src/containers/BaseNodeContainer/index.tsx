import { useState } from 'react'

import BaseNode from './BaseNode'
import { Network } from './types'

const BaseNodeContainer = () => {
  const [tariNetwork, setTariNetwork] = useState<Network>('mainnet')
  const [dark, setDark] = useState(false)
  const toggleDarkMode = () => setDark(a => !a)

  const startNode = () => console.log('start')
  const stopNode = () => console.log('stop')

  return (
    <>
      <button onClick={toggleDarkMode}>toggle dark mode</button>
      <BaseNode
        running={dark}
        startNode={startNode}
        stopNode={stopNode}
        tariNetwork={tariNetwork}
        setTariNetwork={setTariNetwork}
      />
    </>
  )
}

export default BaseNodeContainer
