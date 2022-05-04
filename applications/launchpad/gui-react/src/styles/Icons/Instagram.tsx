import * as React from 'react'
import { SVGProps } from 'react'

const SvgInstagram = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-instagram'
    {...props}
  >
    <path
      d='M3.353 8.95A7.511 7.511 0 0 1 8.95 3.353c2.006-.47 4.094-.47 6.1 0a7.511 7.511 0 0 1 5.597 5.597c.47 2.006.47 4.094 0 6.1a7.511 7.511 0 0 1-5.597 5.597c-2.006.47-4.094.47-6.1 0a7.511 7.511 0 0 1-5.597-5.597 13.354 13.354 0 0 1 0-6.1Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <rect
      x={9}
      y={9}
      width={6}
      height={6}
      rx={3}
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <circle cx={16} cy={7} r={1} fill='currentColor' />
  </svg>
)

export default SvgInstagram
