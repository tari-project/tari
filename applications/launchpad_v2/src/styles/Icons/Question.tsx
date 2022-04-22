import * as React from 'react'
import { SVGProps } from 'react'

const SvgQuestion = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    {...props}
  >
    <path
      d='M8.95 20.647a7.511 7.511 0 0 1-5.597-5.597 13.354 13.354 0 0 1 0-6.1A7.511 7.511 0 0 1 8.95 3.353c2.006-.47 4.094-.47 6.1 0a7.511 7.511 0 0 1 5.597 5.597c.47 2.006.47 4.094 0 6.1a7.511 7.511 0 0 1-5.597 5.597c-2.006.47-4.094.47-6.1 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <circle cx={12} cy={15.5} r={1} fill='currentColor' />
    <path
      d='M10 10v-.5a2 2 0 0 1 2-2v0a2 2 0 0 1 2 2v.121c0 .563-.223 1.102-.621 1.5L12 12.5'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgQuestion
