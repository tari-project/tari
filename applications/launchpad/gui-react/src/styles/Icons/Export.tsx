import * as React from 'react'
import { SVGProps } from 'react'

const SvgExport = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-export'
    {...props}
  >
    <path
      d='M12 3c-1.023 0-2.047.118-3.05.353A7.511 7.511 0 0 0 3.353 8.95a13.354 13.354 0 0 0 0 6.1 7.511 7.511 0 0 0 5.597 5.597c2.006.47 4.094.47 6.1 0a7.511 7.511 0 0 0 5.597-5.597c.235-1.003.353-2.027.353-3.05'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
    <path
      d='M17 3h4m0 0v4.667M21 3l-6 7'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgExport
