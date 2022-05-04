import * as React from 'react'
import { SVGProps } from 'react'

const SvgGrid2 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-grid2'
    {...props}
  >
    <path
      d='M3.678 8.88h16.644M8.88 9.4v10.922M3.353 15.05a13.354 13.354 0 0 1 0-6.1A7.511 7.511 0 0 1 8.95 3.353c2.006-.47 4.094-.47 6.1 0a7.511 7.511 0 0 1 5.597 5.597c.47 2.006.47 4.094 0 6.1a7.511 7.511 0 0 1-5.597 5.597c-2.006.47-4.094.47-6.1 0a7.511 7.511 0 0 1-5.597-5.597Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgGrid2
