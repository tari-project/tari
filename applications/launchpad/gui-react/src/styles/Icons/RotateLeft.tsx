import * as React from 'react'
import { SVGProps } from 'react'

const SvgRotateLeft = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-rotateleft'
    {...props}
  >
    <path
      d='M3 12.152C3 17.04 7.03 21 12 21s9-3.961 9-8.848c0-4.886-4-8.847-9-8.847-6 0-9 4.915-9 4.915m0 0V3m0 5.22h4.655'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgRotateLeft
