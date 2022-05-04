import * as React from 'react'
import { SVGProps } from 'react'

const SvgUserPlus = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-userplus'
    {...props}
  >
    <path
      d='M21 12h-4m2 2v-4'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
    <path
      d='M3 19.111c0-2.413 1.697-4.468 4.004-4.848l.208-.035a17.134 17.134 0 0 1 5.576 0l.208.035c2.307.38 4.004 2.435 4.004 4.848C17 20.154 16.181 21 15.172 21H4.828C3.818 21 3 20.154 3 19.111ZM14.083 6.938c0 2.174-1.828 3.937-4.083 3.937S5.917 9.112 5.917 6.937C5.917 4.764 7.745 3 10 3s4.083 1.763 4.083 3.938Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgUserPlus
