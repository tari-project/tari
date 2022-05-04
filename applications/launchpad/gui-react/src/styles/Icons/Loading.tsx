import * as React from 'react'
import { SVGProps } from 'react'

const SvgLoading = (props: SVGProps<SVGSVGElement>) => (
  <svg
    xmlns='http://www.w3.org/2000/svg'
    width='1em'
    height='1em'
    viewBox='0 0 20 20'
    fill='none'
    data-testid='svg-loading'
    {...props}
  >
    <path
      d='M14 6.00023L16.3642 3.63609M3.63631 16.364L6.00026 14M15.6566 10H19M1 10H4.34315M10 4.34342L10 1M10 19L10 15.6569M6.00023 6.00023L3.63609 3.63609M16.364 16.364L14 14'
      stroke='currentColor'
      strokeWidth='1.5'
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgLoading
