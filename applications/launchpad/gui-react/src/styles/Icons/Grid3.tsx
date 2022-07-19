import * as React from 'react'
import { SVGProps } from 'react'

const SvgGrid3 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-grid3'
    {...props}
  >
    <path
      d='M3.678 8.88H12m0 6.24h8.322M12 3.679v16.644M3.353 15.05a13.354 13.354 0 0 1 0-6.1A7.511 7.511 0 0 1 8.95 3.353c2.006-.47 4.094-.47 6.1 0a7.511 7.511 0 0 1 5.597 5.597c.47 2.006.47 4.094 0 6.1a7.511 7.511 0 0 1-5.597 5.597c-2.006.47-4.094.47-6.1 0a7.511 7.511 0 0 1-5.597-5.597Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgGrid3
