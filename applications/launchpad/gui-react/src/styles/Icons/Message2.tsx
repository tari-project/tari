import * as React from 'react'
import { SVGProps } from 'react'

const SvgMessage2 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-message2'
    {...props}
  >
    <path
      d='M7.926 6.392a6.218 6.218 0 0 0-4.634 4.634c-.39 1.66-.39 3.388 0 5.048a6.218 6.218 0 0 0 4.634 4.634c1.66.39 3.388.39 5.048 0a6.218 6.218 0 0 0 4.634-4.634M7.926 6.392c1.66-.39 3.388-.39 5.048 0a6.218 6.218 0 0 1 4.634 4.634c.39 1.66.39 3.388 0 5.048M7.926 6.392a6.22 6.22 0 0 0-1.145.39 6.218 6.218 0 0 1 4.245-3.49c1.66-.39 3.388-.39 5.048 0a6.218 6.218 0 0 1 4.634 4.634c.39 1.66.39 3.388 0 5.048a6.219 6.219 0 0 1-3.49 4.245 6.22 6.22 0 0 0 .39-1.145'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='m7.5 12 1.89 1.26a2 2 0 0 0 2.22 0L13.5 12'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgMessage2
