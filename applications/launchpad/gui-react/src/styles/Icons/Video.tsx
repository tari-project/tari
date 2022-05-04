import * as React from 'react'
import { SVGProps } from 'react'

const SvgVideo = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-video'
    {...props}
  >
    <path
      d='M2.944 8.284a4.493 4.493 0 0 1 3.323-3.372 14.631 14.631 0 0 1 6.924.007c1.641.401 2.945 1.674 3.358 3.302l.025.099a14.963 14.963 0 0 1 0 7.36l-.025.099c-.413 1.628-1.717 2.901-3.358 3.302a14.63 14.63 0 0 1-6.924.007 4.493 4.493 0 0 1-3.323-3.372l-.062-.274a15.677 15.677 0 0 1 0-6.884l.062-.274Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='m16.663 15.308.197.066c.052.016.101.037.15.061l1.684.836c1.289.64 2.806-.29 2.806-1.723V9.81c0-1.513-1.675-2.435-2.967-1.633l-1.486.922a1.142 1.142 0 0 0-.202.16l-.055.054c.363 1.985.32 4.024-.127 5.995Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgVideo
