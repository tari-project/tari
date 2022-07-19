import * as React from 'react'
import { SVGProps } from 'react'

const SvgArrowBottom2 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-arrowbottom2'
    {...props}
  >
    <path
      d='M12 6.5v11m0 0 4-4.588M12 17.5l-4-4.588'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgArrowBottom2
