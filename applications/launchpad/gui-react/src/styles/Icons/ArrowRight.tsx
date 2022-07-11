import * as React from 'react'
import { SVGProps } from 'react'

const SvgArrowRight = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 20 20'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-arrowright'
    {...props}
  >
    <path
      d='M6.5 12h11m0 0-4.588-4m4.588 4-4.588 4'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgArrowRight
