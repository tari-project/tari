import * as React from 'react'
import { SVGProps } from 'react'

const SvgBag2 = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-bag2'
    {...props}
  >
    <path
      d='M9.2 7 9 7.479a4.13 4.13 0 0 0-.234 2.448 3.139 3.139 0 0 0 2.52 2.445l.111.02c.4.072.809.072 1.208 0l.111-.02a3.14 3.14 0 0 0 2.52-2.445 4.13 4.13 0 0 0-.234-2.448l-.2-.479'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
    <path
      d='M20.22 15.143a6.784 6.784 0 0 1-5.018 5.082c-2.1.509-4.304.509-6.405 0a6.784 6.784 0 0 1-5.018-5.082c-.51-2.24-.386-4.578.358-6.752l.11-.323a7.005 7.005 0 0 1 5.347-4.62l.68-.127a9.431 9.431 0 0 1 3.451 0l.68.127a7.005 7.005 0 0 1 5.347 4.62l.11.323c.744 2.174.868 4.512.358 6.752Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgBag2
