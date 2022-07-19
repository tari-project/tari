import * as React from 'react'
import { SVGProps } from 'react'

const SvgMouse = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-mouse'
    {...props}
  >
    <path
      d='M9.084 20.622a6.43 6.43 0 0 1-4.653-4.907l-.095-.456a15.977 15.977 0 0 1 0-6.518l.095-.456a6.43 6.43 0 0 1 4.653-4.907 11.422 11.422 0 0 1 5.832 0 6.43 6.43 0 0 1 4.653 4.907l.095.456c.448 2.15.448 4.369 0 6.518l-.095.456a6.43 6.43 0 0 1-4.653 4.907c-1.911.504-3.92.504-5.832 0Z'
      stroke='currentColor'
      strokeWidth={1.5}
    />
    <path
      d='M12 7v2'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
    />
  </svg>
)

export default SvgMouse
