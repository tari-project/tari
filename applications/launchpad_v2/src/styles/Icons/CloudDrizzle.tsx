import * as React from 'react'
import { SVGProps } from 'react'

const SvgCloudDrizzle = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-clouddrizzle'
    {...props}
  >
    <path
      d='M17.136 20C20.042 20 22 17.612 22 14.667c0-2.2-1.315-4.09-3.191-4.905C18.466 6.527 15.549 4 12 4c-3.779 0-6.842 2.865-6.842 6.4 0 .09.002.181.006.271A4.798 4.798 0 0 0 2 15.2C2 17.851 3.828 20 6.444 20M10.7 16l-1 1.732m4.464.268-1 1.732m-2.232-.134-1 1.732'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgCloudDrizzle
