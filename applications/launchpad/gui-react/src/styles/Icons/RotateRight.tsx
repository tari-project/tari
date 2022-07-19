import * as React from 'react'
import { SVGProps } from 'react'

const SvgRotateRight = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-rotateright'
    {...props}
  >
    <path
      d='M21 12.152C21 17.04 16.97 21 12 21s-9-3.961-9-8.848c0-4.886 4-8.847 9-8.847 6 0 9 4.915 9 4.915m0 0V3m0 5.22h-4.655'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgRotateRight
