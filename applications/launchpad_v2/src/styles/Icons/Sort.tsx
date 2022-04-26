import * as React from 'react'
import { SVGProps } from 'react'

const SvgSort = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-sort'
    {...props}
  >
    <path
      d='M16 12H8M18 7H6M10 17h4'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgSort
