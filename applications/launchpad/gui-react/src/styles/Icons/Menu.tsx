import * as React from 'react'
import { SVGProps } from 'react'

const SvgMenu = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-menu'
    {...props}
  >
    <path
      d='M7.283 19H20m0-7H4m16-7h-7.028'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgMenu
