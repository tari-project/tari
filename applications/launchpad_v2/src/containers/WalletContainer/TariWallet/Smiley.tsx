import { SVGProps } from 'react'

import SmileyIcon from '../../../styles/Icons/Smiley'
import SmileyNot from '../../../styles/Icons/SmileyNot'

const Smiley = ({
  on,
  ...props
}: { on: boolean } & SVGProps<SVGSVGElement>) => {
  if (on) {
    return <SmileyIcon {...props} width='24px' height='24px' />
  }

  return <SmileyNot {...props} width='24px' height='24px' />
}

export default Smiley
