import * as React from 'react'
import { SVGProps } from 'react'

const SvgTurnoff = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-turnoff'
    {...props}
  >
    <path
      d='M18.364 5.364a9.212 9.212 0 0 1 2.463 4.69 9.31 9.31 0 0 1-.512 5.292A9.126 9.126 0 0 1 17 19.456 8.889 8.889 0 0 1 12 21c-1.78 0-3.52-.537-5-1.544a9.126 9.126 0 0 1-3.315-4.11 9.31 9.31 0 0 1-.512-5.292 9.21 9.21 0 0 1 2.463-4.69M12 3v4.727'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgTurnoff
