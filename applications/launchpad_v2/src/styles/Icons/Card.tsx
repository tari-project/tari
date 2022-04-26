import * as React from 'react'
import { SVGProps } from 'react'

const SvgCard = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-card'
    {...props}
  >
    <path
      d='M3.203 8.7h17.594M2.885 15.151a13.077 13.077 0 0 1 0-6.302 7.353 7.353 0 0 1 5.546-5.407l.453-.101a14.401 14.401 0 0 1 6.232 0l.453.1a7.353 7.353 0 0 1 5.546 5.408c.514 2.07.514 4.233 0 6.302a7.353 7.353 0 0 1-5.546 5.407l-.453.101a14.402 14.402 0 0 1-6.232 0l-.453-.1a7.353 7.353 0 0 1-5.546-5.408Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M7 12h4'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgCard
