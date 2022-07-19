import * as React from 'react'
import { SVGProps } from 'react'

const SvgMarking = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-marking'
    {...props}
  >
    <path
      d='M15.25 12a3.25 3.25 0 1 1-6.5 0 3.25 3.25 0 0 1 6.5 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M21 12h-3M3 12h3M12 21v-3m0-15v3M12 3v3M12 21v-3'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgMarking
