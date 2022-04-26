import * as React from 'react'
import { SVGProps } from 'react'

const SvgLoading = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-loading'
    {...props}
  >
    <path
      d='m16 8 2.364-2.364M5.636 18.364 8 16m9.657-4H21M3 12h3.343M12 6.343V3m0 18v-3.343M8 8 5.636 5.636m12.728 12.728L16 16'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgLoading
