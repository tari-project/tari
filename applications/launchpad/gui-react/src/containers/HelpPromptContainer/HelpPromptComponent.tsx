import TBotPrompt from '../../components/TBot/TBotPrompt'
import { HelpPromptComponentProps } from './types'

const HelpPromptComponent = ({
  open,
  onClose,
  pending = true,
}: HelpPromptComponentProps) => {

  return <TBotPrompt open={open} onClose={onClose} />
}

export default HelpPromptComponent
