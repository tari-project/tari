import * as React from 'react'
import { SVGProps } from 'react'

const SvgArrowSwap = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-arrowswap'
    {...props}
  >
    <path
      d='M5 10.09h14L13.16 5M19 13.91H5L10.84 19'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgArrowSwap
