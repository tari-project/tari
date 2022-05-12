import { useState } from 'react'
import HelpPromptComponent from './HelpPromptComponent'

const HelpPromptContainer = () => {
  const [promptOpen, setPromptOpen] = useState<boolean>(false)

  return <HelpPromptComponent open={promptOpen} onClose={() => setPromptOpen(false)} />
}

export default HelpPromptContainer
