import * as React from 'react'
import { SVGProps } from 'react'

const SvgHeart = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-heart'
    {...props}
  >
    <path
      d='m3.984 11.61 5.134 6.9c1.477 1.986 4.287 1.986 5.764 0l5.134-6.9c1.312-1.763 1.312-4.268 0-6.03-1.92-2.582-6.359-1.815-8.016.969-1.657-2.784-6.096-3.55-8.016-.97-1.312 1.763-1.312 4.268 0 6.032Z'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgHeart
