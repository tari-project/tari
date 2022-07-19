import * as React from 'react'
import { SVGProps } from 'react'

const SvgGps = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-gps'
    {...props}
  >
    <path
      d='M12.318 3.021c.965.128 1.687.95 1.687 1.924v1.383a6.031 6.031 0 0 1 3.667 3.667h1.383c.973 0 1.796.722 1.924 1.687.028.211.028.425 0 .636a1.941 1.941 0 0 1-1.924 1.687h-1.383a6.031 6.031 0 0 1-3.667 3.667v1.383c0 .973-.722 1.796-1.687 1.924a2.422 2.422 0 0 1-.636 0 1.941 1.941 0 0 1-1.687-1.924v-1.383a6.031 6.031 0 0 1-3.667-3.667H4.945a1.941 1.941 0 0 1-1.924-1.687 2.426 2.426 0 0 1 0-.636 1.941 1.941 0 0 1 1.924-1.687h1.383a6.03 6.03 0 0 1 3.667-3.667V4.945c0-.973.722-1.796 1.687-1.924.211-.028.425-.028.636 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
    <path
      d='M9.5 12a2.5 2.5 0 1 1 5 0 2.5 2.5 0 0 1-5 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
  </svg>
)

export default SvgGps
