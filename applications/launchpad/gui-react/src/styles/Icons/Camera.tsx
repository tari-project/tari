import * as React from 'react'
import { SVGProps } from 'react'

const SvgCamera = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-camera'
    {...props}
  >
    <path
      d='M8.44 5.762C8.815 4.18 10.267 3 12 3s3.186 1.18 3.56 2.762l.155.031C18.1 6.28 20 8.038 20.622 10.338c.504 1.867.504 3.83 0 5.696-.622 2.3-2.522 4.06-4.907 4.545l-.456.093c-2.15.437-4.369.437-6.518 0l-.456-.093C5.9 20.094 4 18.334 3.378 16.034a10.905 10.905 0 0 1 0-5.696C4 8.038 5.9 6.278 8.285 5.793l.156-.031Z'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinejoin='round'
    />
    <path
      d='M9.5 13a2.5 2.5 0 1 1 5 0 2.5 2.5 0 0 1-5 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgCamera
