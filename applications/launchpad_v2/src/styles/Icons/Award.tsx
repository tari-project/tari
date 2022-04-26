import * as React from 'react'
import { SVGProps } from 'react'

const SvgAward = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-award'
    {...props}
  >
    <path
      d='M14.203 15.344a5.5 5.5 0 0 0 1.977-.884l1.146 4.507c.345 1.353-1.103 2.478-2.392 1.859l-1.933-.929a2.32 2.32 0 0 0-2.002 0l-1.933.929c-1.29.619-2.737-.506-2.392-1.86L7.82 14.46a5.5 5.5 0 0 0 1.978.884m4.405 0a9.95 9.95 0 0 1-4.405 0m4.405 0c2.005-.456 3.572-1.973 4.042-3.915a9.052 9.052 0 0 0 0-4.267c-.47-1.943-2.037-3.46-4.043-3.915a9.95 9.95 0 0 0-4.404 0c-2.006.456-3.573 1.972-4.043 3.915a9.055 9.055 0 0 0 0 4.266c.47 1.943 2.037 3.46 4.043 3.916'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgAward
