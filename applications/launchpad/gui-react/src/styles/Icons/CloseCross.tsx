import * as React from 'react'
import { SVGProps } from 'react'

const SvgCloseCross = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='6'
    height='6'
    viewBox='0 0 6 6'
    fill='none'
    data-testid='svg-closecross'
    {...props}
  >
    <path
      d='M4.76796 1.23242L1.23242 4.76796M4.76796 4.76796L1.23242 1.23242'
      stroke='currentColor'
      strokeWidth='1.5'
      strokeLinecap='round'
    />
  </svg>
)

export default SvgCloseCross
