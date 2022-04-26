import * as React from 'react'
import { SVGProps } from 'react'

const SvgCloud = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-cloud'
    {...props}
  >
    <path
      d='M6.667 10.318C4.089 10.318 2 12.373 2 14.908 2 17.446 4.09 19.5 6.667 19.5h8.666c3.682 0 6.667-2.936 6.667-6.559 0-3.622-2.985-6.558-6.667-6.558a6.673 6.673 0 0 0-6.1 3.907m-2.566.028c.608 0 1.189.114 1.722.322.322.127.699-.026.845-.35m-2.567.028c-.656 0-1.28.133-1.847.373a4.134 4.134 0 0 1-.598-2.153c0-2.23 1.741-4.038 3.89-4.038 1.752 0 3.234 1.204 3.72 2.86a6.619 6.619 0 0 0-2.598 2.93'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgCloud
