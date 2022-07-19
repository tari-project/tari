import * as React from 'react'
import { SVGProps } from 'react'

const SvgCheckRound = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 17 16'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-checkround'
    {...props}
  >
    <path
      d='M8 10.5a.5.5 0 0 1-.355-.145l-2-2a.502.502 0 0 1 .71-.71L8 9.295l3.145-3.15a.502.502 0 1 1 .71.71l-3.5 3.5A.501.501 0 0 1 8 10.5Z'
      fill='currentColor'
    />
    <path
      d='M8.5 14.5a6.5 6.5 0 1 1 0-13 6.5 6.5 0 0 1 0 13Zm0-12a5.5 5.5 0 1 0 0 11 5.5 5.5 0 0 0 0-11Z'
      fill='currentColor'
    />
  </svg>
)

export default SvgCheckRound
