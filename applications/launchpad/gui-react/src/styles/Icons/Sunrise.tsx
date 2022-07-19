import * as React from 'react'
import { SVGProps } from 'react'

const SvgSunrise = (props: SVGProps<SVGSVGElement>) => (
  <svg
    width='1em'
    height='1em'
    viewBox='0 0 24 24'
    fill='none'
    xmlns='http://www.w3.org/2000/svg'
    data-testid='svg-sunrise'
    {...props}
  >
    <path
      d='M7.143 15.143h-.75v.75h.75v-.75Zm9.714 0v.75h.75v-.75h-.75ZM3.5 14.393a.75.75 0 1 0 0 1.5v-1.5Zm17 1.5a.75.75 0 0 0 0-1.5v1.5ZM7.143 18.25a.75.75 0 0 0 0 1.5v-1.5Zm9.714 1.5a.75.75 0 1 0 0-1.5v1.5Zm-8.964-4.607c0-2.467 1.878-4.393 4.107-4.393v-1.5c-3.137 0-5.607 2.68-5.607 5.893h1.5ZM12 10.75c2.229 0 4.107 1.926 4.107 4.393h1.5c0-3.214-2.47-5.893-5.607-5.893v1.5Zm-4.857 5.143h9.714v-1.5H7.143v1.5Zm-3.643 0h17v-1.5h-17v1.5Zm3.643 3.857h9.714v-1.5H7.143v1.5Z'
      fill='currentColor'
    />
    <path
      d='M12 6V5m4.5 2.062L15.562 8M8.438 8 7.5 7.062'
      stroke='currentColor'
      strokeWidth={1.5}
      strokeLinecap='round'
      strokeLinejoin='round'
    />
  </svg>
)

export default SvgSunrise
