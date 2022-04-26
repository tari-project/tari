import * as React from 'react'
import { SVGProps } from 'react'

const SvgLocation = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-location'
    {...props}
  >
    <path
      d='M20 11.175C20 15.691 16.418 21 12 21s-8-5.31-8-9.825S7.582 3 12 3s8 3.66 8 8.175Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M9.5 10.5a2.5 2.5 0 1 1 5 0 2.5 2.5 0 0 1-5 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgLocation
